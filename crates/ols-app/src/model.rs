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

/// A saved project line shown for comparison: resolved and solved in
/// every weather case of the model.
#[derive(Debug)]
pub struct OtherLine {
    /// Its name.
    pub name: String,
    /// The line (structure types resolved).
    pub line: Line,
    /// Solved in each weather case of the model (same order).
    pub cases: Vec<Solved>,
    /// Display colour.
    pub color: [u8; 3],
}

/// Colours of the other project lines, in turn.
const OTHER_COLORS: [[u8; 3]; 4] = [
    [230, 120, 230],
    [120, 230, 140],
    [240, 170, 60],
    [120, 200, 255],
];

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
    /// is reported and picked), instead of the cases shown (Sections table).
    pub display_cases: std::collections::BTreeMap<String, Vec<usize>>,
    /// The weather cases shown for the whole line (Display weather cases;
    /// empty: the case passed to the views).
    pub shown_cases: Vec<usize>,
    /// Saved project lines shown next to the working line (Lines menu).
    pub other_lines: Vec<OtherLine>,
    /// LiDAR classes not drawn (Terrain menu).
    pub hidden_classes: std::collections::BTreeSet<u8>,
    /// Points per class of the whole point cloud (loaded or not).
    pub class_counts: Vec<(u8, u64)>,
    /// Working folder of the project's libraries (the project holds them);
    /// `None`: read from their files (a line file, or read-only).
    pub library_dir: Option<PathBuf>,
}

fn dir_of(file: &Path) -> PathBuf {
    file.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// A folder of the libraries shipped with OLS CAD (`libraries/<kind>`):
/// in the folder the app was started in, else next to the program or
/// above it (a build in `target/release` of the source tree).
pub fn default_library(kind: &str) -> PathBuf {
    let rel = Path::new("libraries").join(kind);
    if rel.is_dir() {
        return rel;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .skip(1)
                .map(|d| d.join(&rel))
                .find(|d| d.is_dir())
        })
        .unwrap_or(rel)
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
        default_library("cables")
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
            default_library("structures")
        }
    })
}

/// The insulator assemblies' directory.
pub fn insulators_dir(sources: &Sources) -> PathBuf {
    sources
        .insulators
        .clone()
        .unwrap_or_else(|| default_library("insulators"))
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

/// A LiDAR class of up to this many points is loaded whole (larger ones
/// are thinned to the point budget).
pub const SMALL_CLASS: u64 = 20_000;

/// Which LiDAR points are loaded: every class, a small class (up to
/// [`SMALL_CLASS`] points) whole, every `stride`-th point of the others.
#[derive(Debug)]
pub struct Thinning {
    whole: [bool; 256],
    seen: [usize; 256],
    stride: usize,
}

impl Thinning {
    /// For a cloud of `counts` points per class.
    pub fn new(counts: &[(u8, u64)], stride: usize) -> Self {
        let mut whole = [false; 256];
        for (c, n) in counts {
            whole[*c as usize] = *n <= SMALL_CLASS;
        }
        Self {
            whole,
            seen: [0; 256],
            stride: stride.max(1),
        }
    }

    /// Whether the next point of class `c` is loaded.
    pub fn keep(&mut self, c: u8) -> bool {
        let n = &mut self.seen[c as usize];
        *n += 1;
        self.whole[c as usize] || (*n).is_multiple_of(self.stride)
    }
}

/// Project setting holding the weather case file used.
pub const WEATHER_SETTING: &str = "app.weather";

/// Keeps the weather case file in the project's settings: its name when
/// it is one of the project's own files (in the libraries' working
/// folder), else its absolute path.
pub fn remember_weather(session: &ols_session::Session, path: &Path) {
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let own = p
        .parent()
        .is_some_and(|d| d.ends_with("weather") && d.starts_with(libraries_root()));
    let value = match (own, p.file_name()) {
        (true, Some(name)) => name.to_string_lossy().into_owned(),
        _ => p.display().to_string(),
    };
    if let Err(e) = session.set_setting(WEATHER_SETTING, Some(&value)) {
        log::warn!("remembering the weather case file: {e}");
    }
}

/// The libraries a project keeps: kind (in the project) and file extension.
pub const PROJECT_LIBRARIES: [(&str, &str); 4] = [
    ("cables", "olsw"),
    ("structures", "olss"),
    ("insulators", "olsi"),
    ("weather", "olswc"),
];

/// Where the working copies of projects' libraries go.
fn libraries_root() -> PathBuf {
    let t = std::env::temp_dir();
    std::fs::canonicalize(&t)
        .unwrap_or(t)
        .join("ols-cad-libraries")
}

/// The working folder of a project's libraries (one per project and
/// process): the model reads them there; the project holds them.
pub fn library_workdir(file: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    canon.hash(&mut h);
    let stem = file
        .file_stem()
        .map_or("project".into(), |s| s.to_string_lossy().into_owned());
    libraries_root().join(format!(
        "{stem}-{:012x}-{}",
        h.finish() & 0xffff_ffff_ffff,
        std::process::id()
    ))
}

/// Files of `extension` at `at` (a file, or the files of a directory).
fn files_at(at: &Path, extension: &str) -> Vec<PathBuf> {
    if at.is_file() {
        return vec![at.to_path_buf()];
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(at)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == extension))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The project's libraries: each kind the project does not hold yet is
/// copied into it from where the model would read it (given, next to the
/// project, or the default folders); one given on the command line
/// replaces the project's. Then they are written to a working folder and
/// `sources` points there. Without the right to change the project, the
/// libraries it lacks are read from their files as before.
fn project_libraries(
    sources: &mut Sources,
    session: &ols_session::Session,
    log: &mut dyn FnMut(String),
) -> Result<PathBuf, String> {
    let user = sources
        .user
        .clone()
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_default();
    // The weather file used last, when none is given and it is a file.
    let remembered = session.setting(WEATHER_SETTING).ok().flatten();
    let work = library_workdir(&sources.file);
    let _ = std::fs::remove_dir_all(&work);
    for (kind, ext) in PROJECT_LIBRARIES {
        let given = match kind {
            "cables" => sources.library.is_some(),
            "structures" => sources.structures.is_some(),
            "insulators" => sources.insulators.is_some(),
            _ => sources.weather.is_some(),
        };
        let stored = session.library_files(kind).map_err(|e| e.to_string())?;
        let mut kept_in_files = false;
        if stored.is_empty() || given {
            let from: Option<PathBuf> = match kind {
                "cables" => Some(library_path(sources)),
                "structures" => Some(structures_dir(sources)),
                "insulators" => Some(insulators_dir(sources)),
                _ => sources.weather.clone().or_else(|| {
                    remembered
                        .as_deref()
                        .map(PathBuf::from)
                        .filter(|p| p.is_absolute() && p.is_file())
                }),
            };
            let files = from
                .as_deref()
                .map(|f| files_at(f, ext))
                .unwrap_or_default();
            if !files.is_empty() {
                let copy = || -> Result<(), String> {
                    // Given: replaces the project's (weather: adds a file).
                    if given && kind != "weather" {
                        for (name, _) in &stored {
                            session
                                .delete_library_file(&user, kind, name)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    for f in &files {
                        let text = std::fs::read_to_string(f)
                            .map_err(|e| format!("{}: {e}", f.display()))?;
                        let name = f.file_name().map(|n| n.to_string_lossy().into_owned());
                        session
                            .put_library_file(&user, kind, &name.unwrap_or_default(), &text)
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                };
                match copy() {
                    Ok(()) => log(format!(
                        "{kind}: {} file(s) copied into the project from {}",
                        files.len(),
                        from.as_deref().unwrap_or(Path::new("")).display()
                    )),
                    Err(e) => {
                        log(format!("{kind}: read from its files ({e})"));
                        kept_in_files = true;
                    }
                }
            }
        }
        if kept_in_files {
            continue;
        }
        let stored = session.library_files(kind).map_err(|e| e.to_string())?;
        if stored.is_empty() {
            continue;
        }
        let dir = work.join(kind);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (name, text) in &stored {
            std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"))?;
        }
        match kind {
            "cables" => sources.library = Some(dir),
            "structures" => sources.structures = Some(dir),
            "insulators" => sources.insulators = Some(dir),
            _ => {
                // The file given, else the one used last, else the first.
                let wanted = sources
                    .weather
                    .as_deref()
                    .or(remembered.as_deref().map(Path::new))
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                let name = wanted
                    .filter(|n| stored.iter().any(|(s, _)| s == n))
                    .unwrap_or_else(|| stored[0].0.clone());
                sources.weather = Some(dir.join(name));
            }
        }
        log(format!("{kind}: {} file(s) from the project", stored.len()));
    }
    Ok(work)
}

impl Model {
    /// Keeps the working copies of the project's libraries in the project
    /// (after a library page or the weather case table saved one): every
    /// file of the working folder put, files no longer there removed.
    /// Nothing for a line file or libraries read from their files.
    pub fn store_libraries(&self) -> Result<usize, String> {
        let (Some(work), Ok(e)) = (&self.library_dir, &self.editing) else {
            return Ok(0);
        };
        let mut n = 0;
        for (kind, ext) in PROJECT_LIBRARIES {
            let dir = work.join(kind);
            if !dir.is_dir() {
                continue;
            }
            let files = files_at(&dir, ext);
            let names: Vec<String> = files
                .iter()
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            for (f, name) in files.iter().zip(&names) {
                let text = std::fs::read_to_string(f).map_err(|e| format!("{name}: {e}"))?;
                e.session
                    .put_library_file(&e.user, kind, name, &text)
                    .map_err(|e| e.to_string())?;
                n += 1;
            }
            for (name, _) in e.session.library_files(kind).map_err(|e| e.to_string())? {
                if !names.contains(&name) {
                    e.session
                        .delete_library_file(&e.user, kind, &name)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(n)
    }

    /// Writes the project's libraries to `dir` (a folder per kind), to
    /// share them or compare them with other data.
    pub fn export_libraries(&self, dir: &Path) -> Result<usize, String> {
        let e = self.editing.as_ref().map_err(Clone::clone)?;
        let mut n = 0;
        for (kind, _) in PROJECT_LIBRARIES {
            let files = e.session.library_files(kind).map_err(|e| e.to_string())?;
            if files.is_empty() {
                continue;
            }
            let to = dir.join(kind);
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            for (name, text) in files {
                std::fs::write(to.join(&name), text).map_err(|e| format!("{name}: {e}"))?;
                n += 1;
            }
        }
        Ok(n)
    }
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
    /// cases, else the cases shown for the line, else `case`.
    pub fn section_cases(&self, section: &str, case: usize) -> Vec<usize> {
        let valid = |v: &Vec<usize>| -> Vec<usize> {
            v.iter()
                .copied()
                .filter(|k| *k < self.cases.len())
                .collect()
        };
        let own = self
            .display_cases
            .get(section)
            .map(valid)
            .unwrap_or_default();
        if !own.is_empty() {
            return own;
        }
        let shown = valid(&self.shown_cases);
        if shown.is_empty() { vec![case] } else { shown }
    }

    /// The weather cases shown for the whole line (at least one).
    pub fn shown(&self, case: usize) -> Vec<usize> {
        let v: Vec<usize> = self
            .shown_cases
            .iter()
            .copied()
            .filter(|k| *k < self.cases.len())
            .collect();
        if v.is_empty() { vec![case] } else { v }
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

    /// Shows saved project line `name` next to the working line (solved
    /// in every weather case).
    pub fn show_line(&mut self, name: &str) -> Result<String, String> {
        if self.other_lines.iter().any(|o| o.name == name) {
            return Ok(String::new());
        }
        let started = Instant::now();
        let e = self.editing.as_ref().map_err(Clone::clone)?;
        let stored = e.session.snapshot(name).map_err(|e| e.to_string())?;
        let line = match &self.types {
            Some((s, i)) => stored.resolve(s, i).map_err(|e| e.to_string())?,
            None => stored,
        };
        let cases: Vec<Solved> = self
            .cases
            .iter()
            .map(|c| Solved::new(&line, &self.library, c.case.clone()))
            .collect();
        let color = OTHER_COLORS[self.other_lines.len() % OTHER_COLORS.len()];
        let failed: usize = cases.iter().map(|c| c.failed.len()).sum();
        self.other_lines.push(OtherLine {
            name: name.to_owned(),
            line,
            cases,
            color,
        });
        Ok(format!(
            "project line \"{name}\" shown, solved in {} weather cases in {:.1?}{}",
            self.cases.len(),
            started.elapsed(),
            if failed > 0 {
                format!(" — {failed} sections or jumpers failed")
            } else {
                String::new()
            }
        ))
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
        // Project lines shown: solved again in the new cases.
        let others: Vec<String> = self.other_lines.drain(..).map(|o| o.name).collect();
        self.line = line;
        self.library = library;
        self.types = types;
        let failed: usize = self.cases.iter().map(|c| c.failed.len()).sum();
        for name in others {
            if let Err(e) = self.show_line(&name) {
                log::warn!("project line {name}: {e}");
            }
        }
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
pub fn load(mut sources: Sources, progress: &dyn Fn(String)) -> Result<Model, String> {
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
    // The project's own libraries.
    let library_dir = match &session {
        Some(s) => Some(project_libraries(&mut sources, s, &mut |m| {
            step(m, &mut log)
        })?),
        None => None,
    };
    let types = load_types(&sources, &line)?;
    if let Some((structures, insulators)) = &types {
        line = line
            .resolve(structures, insulators)
            .map_err(|e| e.to_string())?;
        let dir = structures_dir(&sources);
        step(
            match &library_dir {
                Some(w) if dir.starts_with(w) => "structure types from the project".to_owned(),
                _ => format!("structure types from {}", dir.display()),
            },
            &mut log,
        );
    }
    let lib_path = library_path(&sources);
    let library = ols_cables::Library::load(&lib_path).map_err(|e| e.to_string())?;
    step(
        match &library_dir {
            Some(w) if lib_path.starts_with(w) => "wire library from the project".to_owned(),
            _ => format!("wire library {}", lib_path.display()),
        },
        &mut log,
    );
    // The weather case file the project used last, when none is given.
    if sources.weather.is_none()
        && let Some(s) = &session
        && let Ok(Some(p)) = s.setting(WEATHER_SETTING)
    {
        let p = PathBuf::from(p);
        if p.is_file() {
            step(
                format!("weather cases from {} (used last)", p.display()),
                &mut log,
            );
            sources.weather = Some(p);
        } else {
            step(
                format!("weather case file used last not found: {}", p.display()),
                &mut log,
            );
        }
    }
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
    let mut class_counts: Vec<(u8, u64)> = Vec::new();
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
        // Every class shown, of the whole cloud: a class of up to
        // SMALL_CLASS points whole, the others every n-th point, n from
        // the cloud's size. (Thinning every class alike and leaving out the
        // ground classes, as before, lost whole classes of a real survey:
        // roads, water, bridges, and classes of 6 to 500 points.)
        let info = source.cloud().clone();
        let stride = (info.points as usize / sources.point_budget.max(1)).max(1);
        class_counts = ols_pointcloud::Store::open(Path::new(&file))
            .and_then(|s| s.class_counts(&info))
            .map_err(|e| e.to_string())?;
        let mut thinning = Thinning::new(&class_counts, stride);
        let mut cells: std::collections::HashMap<(i64, i64), usize> =
            std::collections::HashMap::new();
        let mut ground_points: Vec<[f64; 3]> = Vec::new();
        let mm = |v: f64, i: usize| ((v - origin[i]) * 1000.0).round() as i32;
        // Exit clause: decoding is ~1 s per 10⁷ points; 10 min is far beyond.
        let budget =
            ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
        let stats =
            ols_pointcloud::PointSource::visit(&source, None, None, &budget, &mut |x, y, z, c| {
                if thinning.keep(c) {
                    points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c));
                }
                // The ground surface: ground classes in the corridor.
                if is_ground[c as usize] && corridor.contains(x, y) {
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
                }
            })
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
    // Remember the weather case file with the project.
    if let (Ok(e), Some(p)) = (&editing, &sources.weather) {
        remember_weather(&e.session, p);
    }
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
        shown_cases: vec![0],
        other_lines: Vec::new(),
        hidden_classes: Default::default(),
        class_counts,
        library_dir,
    })
}
