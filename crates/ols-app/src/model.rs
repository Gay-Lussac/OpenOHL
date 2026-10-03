// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Loading a model for the views: the line (from a line file or a project),
//! its structure types and libraries, the weather cases solved, the ground
//! surface and LiDAR points. Runs on a background thread
//! ([`load`]), reporting progress.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ols_line::Line;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;
use ols_weather::{WeatherCase, WeatherCases};

/// Where the model comes from.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    /// Line file (`.ols.json`) or project (`.ols`).
    pub file: PathBuf,
    /// Wire library (file or directory); default: `*.olsw` next to the
    /// file, else `libraries/cables`.
    pub library: Option<PathBuf>,
    /// Pylon files; default: `Structures/` next to the file, else
    /// `libraries/structures`.
    pub structures: Option<PathBuf>,
    /// Insulator library.
    pub insulators: Option<PathBuf>,
    /// Weather case file (`.olswc`); default: one case at 15 °C.
    pub weather: Option<PathBuf>,
    /// Survey points for the ground (CSV/XYZ).
    pub terrain: Option<PathBuf>,
    /// Point cloud: a project file (`FILE.ols` or `FILE.ols#name`);
    /// default: the project's own, if it has one.
    pub pointcloud: Option<String>,
    /// Most LiDAR points shown (the ground surface uses all ground points).
    pub point_budget: usize,
    /// Half width of the corridor around the spans for LiDAR, m.
    pub corridor: f64,
    /// User editing the project (default: the login name).
    pub user: Option<String>,
    /// Clearance rules (`.olsr`).
    pub rules: Option<PathBuf>,
}

/// One solved weather case.
#[derive(Debug, Clone)]
pub struct Solved {
    /// The case.
    pub case: WeatherCase,
    /// Solved sections (failed ones left out).
    pub shapes: Vec<SectionShape>,
    /// Solved jumpers.
    pub jumpers: Vec<JumperShape>,
    /// Sections or jumpers that failed, with why.
    pub failed: Vec<String>,
    /// Subconductors joined up at the clamps (yokes), for drawing.
    pub bundles: ols_line::bundles::BundleLines,
    /// The solution kept for re-solving only what an edit touches.
    pub live: ols_line::incremental::SolvedLine,
}

impl Solved {
    /// Solves `line` in `case`.
    pub fn new(line: &Line, library: &ols_cables::Library, case: WeatherCase) -> Self {
        let live = ols_line::incremental::SolvedLine::solve(line, library, &case);
        Self::from_live(line, case, live)
    }

    /// The shapes, failures and bundles of a solution.
    pub fn from_live(
        line: &Line,
        case: WeatherCase,
        live: ols_line::incremental::SolvedLine,
    ) -> Self {
        let mut s = Self {
            case,
            shapes: Vec::new(),
            jumpers: Vec::new(),
            failed: Vec::new(),
            bundles: Default::default(),
            live,
        };
        s.refresh(line);
        s
    }

    fn refresh(&mut self, line: &Line) {
        self.shapes = self.live.shapes(line);
        self.jumpers = self.live.jumper_shapes(line);
        self.failed = self
            .live
            .results(line)
            .into_iter()
            .filter_map(|(id, r)| r.as_ref().err().map(|e| format!("{id}: {e}")))
            .collect();
        self.failed.extend(
            self.live
                .jumper_results(line)
                .into_iter()
                .filter_map(|(id, r)| r.as_ref().err().map(|e| format!("{id}: {e}"))),
        );
        self.bundles = ols_line::bundles::BundleLines::new(line, &self.shapes, &self.jumpers);
    }

    /// Re-solves the sections `dirty` of `line` (and every jumper); returns
    /// how many sections were solved.
    pub fn update(
        &mut self,
        line: &Line,
        library: &ols_cables::Library,
        dirty: &[String],
    ) -> usize {
        let u = self.live.update(line, library, dirty);
        self.refresh(line);
        u.solved
    }
}

/// The project being edited.
pub struct Editing {
    /// The project's session.
    pub session: ols_session::Session,
    /// The user editing.
    pub user: String,
}

impl std::fmt::Debug for Editing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editing")
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

/// A LiDAR point: millimetres from the model origin, class.
pub type LidarPoint = ([i32; 3], u8);

/// A loaded model.
#[derive(Debug)]
pub struct Model {
    /// Where it came from.
    pub sources: Sources,
    /// The line (structure types resolved).
    pub line: Line,
    /// Weather cases, solved.
    pub cases: Vec<Solved>,
    /// Ground surface (shared with clearance checks).
    pub ground: Option<std::sync::Arc<Terrain>>,
    /// The point cloud read (`FILE.ols` or `FILE.ols#name`).
    pub cloud: Option<String>,
    /// LiDAR points shown (thinned to the budget).
    pub points: Vec<LidarPoint>,
    /// Origin of the points (and of the views), project coordinates.
    pub origin: [f64; 3],
    /// Class scheme of the points.
    pub scheme: ols_lidar::ClassScheme,
    /// What was loaded, for the status panel.
    pub log: Vec<String>,
    /// Wire library (for re-solving after edits).
    pub library: ols_cables::Library,
    /// Structure and insulator libraries, when the line uses types.
    pub types: Option<(
        ols_structures::StructureLibrary,
        ols_structures::InsulatorLibrary,
    )>,
    /// The project to edit, or why the model is read-only.
    pub editing: Result<Editing, String>,
    /// Sections drawn in their own weather cases (indices; the first one
    /// is reported and picked), instead of the case shown (Sections table).
    pub display_cases: std::collections::BTreeMap<String, Vec<usize>>,
}

fn dir_of(file: &Path) -> PathBuf {
    file.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// The wire library: given, else the `*.olsw` files next to the model,
/// else `libraries/cables`.
pub fn library_path(s: &Sources) -> PathBuf {
    if let Some(p) = &s.library {
        return p.clone();
    }
    let dir = dir_of(&s.file);
    let has_olsw = std::fs::read_dir(&dir).is_ok_and(|mut it| {
        it.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "olsw")))
    });
    if has_olsw {
        dir
    } else {
        PathBuf::from("libraries/cables")
    }
}

/// The line, and the project's session when it is a project file.
fn load_line(path: &Path) -> Result<(Line, Option<ols_session::Session>), String> {
    if path.extension().is_some_and(|e| e == "ols") {
        let s = ols_session::Session::open(path).map_err(|e| e.to_string())?;
        if !s.has_line() {
            return Err(format!("{}: the project holds no line", path.display()));
        }
        Ok((s.export(), Some(s)))
    } else {
        Ok((Line::load(path).map_err(|e| e.to_string())?, None))
    }
}

/// Who edits, and whether they may.
fn editing(session: Option<ols_session::Session>, user: Option<&str>) -> Result<Editing, String> {
    let Some(session) = session else {
        return Err("a line file: open a project (.ols) to edit (ols project import)".into());
    };
    let user = user
        .map(str::to_owned)
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_default();
    match session.role(&user).map_err(|e| e.to_string())? {
        Some(ols_session::Role::Viewer) => Err(format!("user {user} is a viewer of this project")),
        Some(_) => Ok(Editing { session, user }),
        None => Err(format!(
            "user {user} has no role in this project (an admin adds users: ols project user)"
        )),
    }
}

/// The structure types' directory: given, else `Structures/` next to the
/// model, else `libraries/structures`.
pub fn structures_dir(sources: &Sources) -> PathBuf {
    sources.structures.clone().unwrap_or_else(|| {
        let local = dir_of(&sources.file).join("Structures");
        if local.is_dir() {
            local
        } else {
            PathBuf::from("libraries/structures")
        }
    })
}

/// The insulator assemblies' directory.
pub fn insulators_dir(sources: &Sources) -> PathBuf {
    sources
        .insulators
        .clone()
        .unwrap_or_else(|| PathBuf::from("libraries/insulators"))
}

/// The directory next to the model.
pub fn model_dir(sources: &Sources) -> PathBuf {
    dir_of(&sources.file)
}

/// Structure and insulator libraries: when the line has typed structures,
/// or there are types to add.
#[allow(clippy::type_complexity)]
fn load_types(
    sources: &Sources,
    line: &Line,
) -> Result<
    Option<(
        ols_structures::StructureLibrary,
        ols_structures::InsulatorLibrary,
    )>,
    String,
> {
    let dir = structures_dir(sources);
    if !(line.structures.iter().any(|s| s.structure_type.is_some()) || dir.is_dir()) {
        return Ok(None);
    }
    let structures = ols_structures::StructureLibrary::load_dir(&dir).map_err(|e| e.to_string())?;
    let insulators = ols_structures::InsulatorLibrary::load_dir(&insulators_dir(sources))
        .map_err(|e| e.to_string())?;
    Ok(Some((structures, insulators)))
}

/// The weather cases: from the file given, else still air at 15 °C.
fn load_cases(sources: &Sources) -> Result<Vec<WeatherCase>, String> {
    Ok(match &sources.weather {
        Some(p) => WeatherCases::load(p)?.cases,
        None => vec![WeatherCase::still(
            "15 °C",
            ols_core::units::Temperature::from_celsius(15.0),
        )],
    })
}

impl Model {
    /// The weather cases section `section` is drawn in: its own display
    /// cases, else `case`.
    pub fn section_cases(&self, section: &str, case: usize) -> Vec<usize> {
        let own: Vec<usize> = self
            .display_cases
            .get(section)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|k| *k < self.cases.len())
                    .collect()
            })
            .unwrap_or_default();
        if own.is_empty() { vec![case] } else { own }
    }

    /// The weather case section `section` is reported and picked in: its
    /// first display case, else `case`.
    pub fn section_case(&self, section: &str, case: usize) -> usize {
        self.section_cases(section, case)[0]
    }

    /// Section `section` as shown (its display case, else `case`).
    pub fn shape(&self, section: &str, case: usize) -> Option<&SectionShape> {
        self.cases
            .get(self.section_case(section, case))?
            .shapes
            .iter()
            .find(|s| s.section == section)
    }

    /// Every section in each weather case it is drawn in, in line order.
    pub fn shapes_shown(&self, case: usize) -> Vec<SectionShape> {
        self.line
            .sections
            .iter()
            .flat_map(|s| {
                self.section_cases(&s.id, case).into_iter().filter_map(|k| {
                    self.cases
                        .get(k)?
                        .shapes
                        .iter()
                        .find(|x| x.section == s.id)
                        .cloned()
                })
            })
            .collect()
    }

    /// Reads the libraries again (wires, structure types, insulators,
    /// weather cases) after one was edited, resolves the line again and
    /// solves every case. Nothing changes when one does not load.
    pub fn reload_libraries(&mut self) -> Result<String, String> {
        let started = Instant::now();
        let stored = match &self.editing {
            Ok(e) => e.session.export(),
            Err(_) => load_line(&self.sources.file)?.0,
        };
        let types = load_types(&self.sources, &stored)?;
        let line = match &types {
            Some((s, i)) => stored.resolve(s, i).map_err(|e| e.to_string())?,
            None => stored,
        };
        let library =
            ols_cables::Library::load(&library_path(&self.sources)).map_err(|e| e.to_string())?;
        let cases = load_cases(&self.sources)?;
        self.cases = cases
            .into_iter()
            .map(|c| Solved::new(&line, &library, c))
            .collect();
        self.line = line;
        self.library = library;
        self.types = types;
        let failed: usize = self.cases.iter().map(|c| c.failed.len()).sum();
        Ok(format!(
            "libraries read again, {} weather cases solved in {:.1?}{}",
            self.cases.len(),
            started.elapsed(),
            if failed > 0 {
                format!(" — {failed} sections or jumpers failed")
            } else {
                String::new()
            }
        ))
    }
}

/// Loads and solves everything; `progress` gets a line per step.
pub fn load(sources: Sources, progress: &dyn Fn(String)) -> Result<Model, String> {
    let mut log = Vec::new();
    let step = |m: String, log: &mut Vec<String>| {
        progress(m.clone());
        log.push(m);
    };
    let started = Instant::now();
    let (mut line, session) = load_line(&sources.file)?;
    step(
        format!(
            "{}: {} structures, {} sections, {} jumpers",
            line.name,
            line.structures.len(),
            line.sections.len(),
            line.jumpers.len()
        ),
        &mut log,
    );
    let types = load_types(&sources, &line)?;
    if let Some((structures, insulators)) = &types {
        line = line
            .resolve(structures, insulators)
            .map_err(|e| e.to_string())?;
        step(
            format!(
                "structure types from {}",
                structures_dir(&sources).display()
            ),
            &mut log,
        );
    }
    let lib_path = library_path(&sources);
    let library = ols_cables::Library::load(&lib_path).map_err(|e| e.to_string())?;
    step(format!("wire library {}", lib_path.display()), &mut log);
    let cases = load_cases(&sources)?;
    let mut solved = Vec::new();
    for case in cases {
        let t = Instant::now();
        let name = case.name.clone();
        let one = Solved::new(&line, &library, case);
        step(
            format!(
                "case \"{name}\" solved in {:.0?}{}",
                t.elapsed(),
                if one.failed.is_empty() {
                    String::new()
                } else {
                    format!(" — {} failed", one.failed.len())
                }
            ),
            &mut log,
        );
        solved.push(one);
    }
    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let mut ground = match &sources.terrain {
        Some(p) => {
            let t = Terrain::load(p).map_err(|e| e.to_string())?;
            step(format!("ground: {} triangles", t.num_triangles()), &mut log);
            Some(t)
        }
        None => None,
    };
    // Point cloud: given, else the project's own.
    let cloud = sources.pointcloud.clone().or_else(|| {
        let is_project = sources.file.extension().is_some_and(|e| e == "ols");
        let has = is_project
            && ols_pointcloud::Store::open(&sources.file)
                .and_then(|s| s.clouds())
                .is_ok_and(|c| !c.is_empty());
        has.then(|| sources.file.display().to_string())
    });
    let mut points = Vec::new();
    let mut scheme = ols_lidar::ClassScheme::asprs();
    if let Some(spec) = cloud.clone() {
        let t = Instant::now();
        let (file, name) = match spec.split_once('#') {
            Some((f, n)) => (f.to_owned(), Some(n.to_owned())),
            None => (spec.clone(), None),
        };
        let source = ols_pointcloud::CloudSource::open(Path::new(&file), name.as_deref())
            .map_err(|e| e.to_string())?;
        if let Some(text) = ols_pointcloud::PointSource::class_scheme(&source) {
            scheme = ols_lidar::ClassScheme::parse(&text)?;
        }
        let ground_classes = scheme.codes_with_role("ground");
        let mut is_ground = [false; 256];
        for &c in &ground_classes {
            is_ground[c as usize] = true;
        }
        // Corridor along the spans' chords.
        let polylines: Vec<Vec<[f64; 2]>> = line
            .sections
            .iter()
            .map(|s| {
                s.points
                    .iter()
                    .filter_map(|p| line.attachment(p))
                    .map(|a| [a.wire_point[0], a.wire_point[1]])
                    .collect()
            })
            .collect();
        let corridor = ols_lidar::Corridor::new(&polylines, sources.corridor);
        // Every n-th point of the other classes, n from the cloud's size.
        let info = source.cloud().clone();
        let stride = (info.points as usize / sources.point_budget.max(1)).max(1);
        let mut cells: std::collections::HashMap<(i64, i64), usize> =
            std::collections::HashMap::new();
        let mut ground_points: Vec<[f64; 3]> = Vec::new();
        let mut k = 0usize;
        let mm = |v: f64, i: usize| ((v - origin[i]) * 1000.0).round() as i32;
        // Exit clause: decoding is ~1 s per 10⁷ points; 10 min is far beyond.
        let budget =
            ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
        let stats = ols_pointcloud::PointSource::visit(
            &source,
            Some(&corridor),
            None,
            &budget,
            &mut |x, y, z, c| {
                if is_ground[c as usize] {
                    // Lowest point per 1 m cell for the surface.
                    let key = (x.floor() as i64, y.floor() as i64);
                    match cells.get(&key) {
                        Some(&i) if ground_points[i][2] <= z => {}
                        Some(&i) => ground_points[i] = [x, y, z],
                        None => {
                            cells.insert(key, ground_points.len());
                            ground_points.push([x, y, z]);
                        }
                    }
                } else {
                    k += 1;
                    if k.is_multiple_of(stride) {
                        points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c));
                    }
                }
            },
        )
        .map_err(|e| e.to_string())?;
        step(
            format!(
                "points: {} decoded in {:.1?}; {} shown, {} ground cells",
                stats.points_visited,
                t.elapsed(),
                points.len(),
                ground_points.len()
            ),
            &mut log,
        );
        if ground.is_none() && ground_points.len() >= 3 {
            let t = Instant::now();
            let g = Terrain::from_xyz(&ground_points, Some(50.0)).map_err(|e| e.to_string())?;
            step(
                format!(
                    "ground: {} triangles from LiDAR in {:.1?}",
                    g.num_triangles(),
                    t.elapsed()
                ),
                &mut log,
            );
            ground = Some(g);
        }
    }
    let editing = editing(session, sources.user.as_deref());
    match &editing {
        Ok(e) => step(format!("editing as {}", e.user), &mut log),
        Err(why) => step(format!("read-only: {why}"), &mut log),
    }
    step(format!("ready in {:.1?}", started.elapsed()), &mut log);
    Ok(Model {
        sources,
        line,
        cases: solved,
        ground: ground.map(std::sync::Arc::new),
        cloud,
        points,
        origin,
        scheme,
        log,
        library,
        types,
        editing,
        display_cases: Default::default(),
    })
}
