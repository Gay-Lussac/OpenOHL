// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Loading a model for the views: the line (from a line file or a project),
//! its structure types and libraries, the weather cases solved, the ground
//! surface and LiDAR points. Runs on a background thread
//! ([`load`]), reporting progress.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ohl_core::files::{dir_of, files_with_extension, shipped_library};
use ohl_line::Line;
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;
use ohl_terrain::Terrain;
use ohl_weather::{WeatherCase, WeatherCases};

/// Where the model comes from.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    /// Line file (`.ohl.json`) or project (`.ohl`).
    pub file: PathBuf,
    /// Wire library (file or directory); default: `*.ohlw` next to the
    /// file, else `libraries/cables`.
    pub library: Option<PathBuf>,
    /// Pylon files; default: `Structures/` next to the file.
    pub structures: Option<PathBuf>,
    /// Insulator library.
    pub insulators: Option<PathBuf>,
    /// Weather case file (`.ohlwc`); default: one case at 15 °C.
    pub weather: Option<PathBuf>,
    /// Survey points for the ground (CSV/XYZ).
    pub terrain: Option<PathBuf>,
    /// Point cloud: a project file (`FILE.ohl` or `FILE.ohl#name`) or a
    /// COPC file (`FILE.copc.laz`);
    /// default: the project's own, if it has one.
    pub pointcloud: Option<String>,
    /// Most LiDAR points shown (the ground surface uses all ground points).
    pub point_budget: usize,
    /// Half width of the corridor around the spans for LiDAR, m.
    pub corridor: f64,
    /// User editing the project (default: the login name).
    pub user: Option<String>,
    /// Clearance rules (`.ohlr`).
    pub rules: Option<PathBuf>,
    /// Load cases (`.ohllc`) for the structure loads; default: the
    /// project's own, else the first `.ohllc` next to the file.
    pub load_cases: Option<PathBuf>,
    /// What this user showed of the file last time ([`ViewPrefs`]).
    pub view: Option<ViewPrefs>,
    /// Class scheme (`.ohlc`: names, roles, colours) used instead of the
    /// point cloud's own (this user's choice).
    pub class_scheme: Option<PathBuf>,
}

/// What one user shows of a project — the ground surface used and the
/// others shown, the LiDAR classes hidden and the share of points shown.
/// Remembered on this computer, per project file; never written to the
/// project nor sent to a server: everyone decides for themselves.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewPrefs {
    /// Terrain surfaces: the ground, others shown.
    #[serde(default)]
    pub surfaces: crate::surfaces::SurfaceSettings,
    /// Share of the point cloud shown near the camera, %.
    #[serde(default)]
    pub lidar_percent: Option<f64>,
    /// LiDAR classes hidden.
    #[serde(default)]
    pub hidden_classes: std::collections::BTreeSet<u8>,
    /// Class scheme file used instead of the point cloud's own.
    #[serde(default)]
    pub class_scheme: Option<PathBuf>,
    /// What is written next to the wires in each view.
    #[serde(default)]
    pub section_display: crate::annotations::SectionDisplay,
    /// The side the wind comes from in the weather cases shown.
    #[serde(default)]
    pub wind: WindDisplay,
    /// How the display weather cases apply to the sections.
    #[serde(default)]
    pub cases_shown: CasesShown,
    /// Backgrounds hidden (`image:<layer>`, `drawing:<name>`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub hidden_backgrounds: std::collections::BTreeSet<String>,
    /// The project line this user works on (Versions › Project lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The project lines shown next to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shown_lines: Vec<String>,
    /// The profile view's heights against its stations (View › Profile
    /// aspect ratio): drawn this many times larger; none: fitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ratio: Option<f64>,
    /// The route worked on (View › Work on route); none: all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_route: Option<String>,
    /// The colours of the vegetation layers: grow-in points, danger trees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vegetation_colors: Option<[[u8; 3]; 2]>,
}

/// The [`ViewPrefs`] of every project opened, by file (remembered).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewPrefsStore(pub std::collections::BTreeMap<String, ViewPrefs>);

impl ViewPrefsStore {
    fn key(file: &Path) -> String {
        std::fs::canonicalize(file)
            .unwrap_or_else(|_| file.to_path_buf())
            .display()
            .to_string()
    }

    /// Those of `file`.
    pub fn get(&self, file: &Path) -> Option<ViewPrefs> {
        self.0.get(&Self::key(file)).cloned()
    }

    /// Keeps those of `file`.
    pub fn set(&mut self, file: &Path, prefs: ViewPrefs) {
        if !file.as_os_str().is_empty() {
            self.0.insert(Self::key(file), prefs);
        }
    }

    /// Saved as JSON.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(t) = serde_json::to_string(&self.0) {
            storage.set_string("view-prefs", t);
        }
    }

    /// As saved.
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        Self(
            storage
                .get_string("view-prefs")
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default(),
        )
    }
}

/// A section not solved in a weather case, drawn in its place (dashed
/// red): [`Model::unsolved_drawn`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnsolvedDrawn {
    /// Each span's points (a span without both wire points: empty).
    pub spans: Vec<Vec<[f64; 3]>>,
    /// Where the shape comes from, in words.
    pub from: String,
    /// Why it did not solve.
    pub reason: String,
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
    /// Sections not solved in this case: (id, why).
    pub unsolved: Vec<(String, String)>,
    /// The shape of each section of [`Solved::unsolved`] from before it
    /// stopped solving (a previous solve of this case), when there was
    /// one: drawn dashed red in its place ([`Model::unsolved_drawn`]).
    pub last_solved: std::collections::BTreeMap<String, SectionShape>,
    /// Subconductors joined up at the clamps (yokes), for drawing.
    pub bundles: ohl_line::bundles::BundleLines,
    /// The solution kept for re-solving only what an edit touches.
    pub live: ohl_line::incremental::SolvedLine,
    /// Solved (else not yet: solved when first shown or checked,
    /// [`Model::want`]).
    pub ready: bool,
    /// The same case with the wind from the other side, when both sides
    /// are shown ([`WindDisplay::Both`]) and the case has wind.
    pub mirror: Option<Box<Solved>>,
}

/// Which side the wind of the weather cases comes from, looking ahead
/// along the line: a display choice (View › Display weather cases),
/// not part of the weather case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindDisplay {
    /// From the right.
    #[default]
    Right,
    /// From the left.
    Left,
    /// Both sides drawn at once (the right one reported, checked, picked).
    Both,
}

impl WindDisplay {
    /// Every choice, in the menu's order.
    pub const ALL: [WindDisplay; 3] = [WindDisplay::Right, WindDisplay::Left, WindDisplay::Both];

    /// Name in the menu.
    pub fn name(self) -> &'static str {
        match self {
            WindDisplay::Right => "from the right",
            WindDisplay::Left => "from the left",
            WindDisplay::Both => "both sides",
        }
    }

    /// The side solved first (reported, checked, picked).
    pub fn primary(self) -> ohl_weather::WindSide {
        match self {
            WindDisplay::Left => ohl_weather::WindSide::Left,
            WindDisplay::Right | WindDisplay::Both => ohl_weather::WindSide::Right,
        }
    }
}

/// How the display weather cases apply to the sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasesShown {
    /// The cases chosen for the line, for every section (the Sections
    /// table's display cases are kept, not used).
    #[default]
    AllSections,
    /// Each section's display cases from the Sections table (the line's
    /// cases for sections without).
    PerSection,
}

impl Solved {
    /// Solves `line` in `case`, and with the wind from the other side too
    /// when `wind` shows both and the case has wind.
    pub fn with_wind(
        line: &Line,
        library: &ohl_cables::Library,
        mut case: WeatherCase,
        wind: WindDisplay,
    ) -> Self {
        case.wind_from = wind.primary();
        let mirrored = (wind == WindDisplay::Both && case.wind_pressure.si() > 0.0).then(|| {
            let mut other = case.clone();
            other.wind_from = ohl_weather::WindSide::Left;
            Box::new(Self::new(line, library, other))
        });
        let mut s = Self::new(line, library, case);
        s.mirror = mirrored;
        s
    }

    /// Whether it is solved as `wind` shows it (its side, its mirror).
    pub fn solved_for(&self, wind: WindDisplay) -> bool {
        let windy = self.case.wind_pressure.si() > 0.0;
        self.ready
            && (!windy
                || (self.case.wind_from == wind.primary()
                    && self.mirror.is_some() == (wind == WindDisplay::Both)))
    }

    /// The solutions drawn: this one, and its mirror if any.
    pub fn drawn(&self) -> impl Iterator<Item = &Solved> {
        std::iter::once(self).chain(self.mirror.as_deref())
    }

    /// Solves `line` in `case`.
    pub fn new(line: &Line, library: &ohl_cables::Library, case: WeatherCase) -> Self {
        let live = ohl_line::incremental::SolvedLine::solve(line, library, &case);
        Self::from_live(line, case, live)
    }

    /// A new solution of the same case replacing `old`: the sections not
    /// solved in this one that `old` had a shape for (solved, or kept from
    /// before) keep that shape ([`Solved::last_solved`]); its mirror the
    /// same from `old`'s mirror.
    pub fn inherit(&mut self, old: &Solved) {
        for (id, _) in &self.unsolved {
            if self.last_solved.contains_key(id) {
                continue;
            }
            let shape = old
                .shapes
                .iter()
                .find(|s| &s.section == id)
                .or_else(|| old.last_solved.get(id));
            if let Some(shape) = shape {
                self.last_solved.insert(id.clone(), shape.clone());
            }
        }
        if let (Some(m), Some(o)) = (self.mirror.as_deref_mut(), old.mirror.as_deref()) {
            m.inherit(o);
        }
    }

    /// `case`, not solved yet (nothing drawn, nothing failed).
    pub fn pending(case: WeatherCase) -> Self {
        Self {
            live: ohl_line::incremental::SolvedLine::empty(&case),
            case,
            shapes: Vec::new(),
            jumpers: Vec::new(),
            failed: Vec::new(),
            unsolved: Vec::new(),
            last_solved: Default::default(),
            bundles: Default::default(),
            ready: false,
            mirror: None,
        }
    }

    /// The shapes, failures and bundles of a solution.
    pub fn from_live(
        line: &Line,
        case: WeatherCase,
        live: ohl_line::incremental::SolvedLine,
    ) -> Self {
        let mut s = Self {
            case,
            shapes: Vec::new(),
            jumpers: Vec::new(),
            failed: Vec::new(),
            unsolved: Vec::new(),
            last_solved: Default::default(),
            bundles: Default::default(),
            live,
            ready: true,
            mirror: None,
        };
        s.refresh(line);
        s
    }

    fn refresh(&mut self, line: &Line) {
        // The shapes as they were: kept for the sections that stop solving.
        let mut before = std::mem::take(&mut self.last_solved);
        for shape in std::mem::take(&mut self.shapes) {
            before.insert(shape.section.clone(), shape);
        }
        self.shapes = self.live.shapes(line);
        self.unsolved = self
            .live
            .results(line)
            .into_iter()
            .filter_map(|(id, r)| r.as_ref().err().map(|e| (id.to_owned(), e.to_string())))
            .collect();
        self.last_solved = self
            .unsolved
            .iter()
            .filter_map(|(id, _)| before.remove(id).map(|shape| (id.clone(), shape)))
            .collect();
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
        self.bundles = ohl_line::bundles::BundleLines::new(line, &self.shapes, &self.jumpers);
    }

    /// Re-solves the sections `dirty` of `line` (and every jumper); returns
    /// how many sections were solved.
    pub fn update(
        &mut self,
        line: &Line,
        library: &ohl_cables::Library,
        dirty: &[String],
    ) -> usize {
        // Not solved yet: solved from the line as it is when wanted.
        if !self.ready {
            return 0;
        }
        let u = self.live.update(line, library, dirty);
        self.refresh(line);
        if let Some(m) = &mut self.mirror {
            m.update(line, library, dirty);
        }
        u.solved
    }
}

/// The project being edited.
pub struct Editing {
    /// The project's session.
    pub session: ohl_session::Session,
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
    /// Where the ground comes from, in words (empty: built at opening).
    pub ground_source: String,
    /// The project's terrain surface settings (active, shown).
    pub surface_settings: crate::surfaces::SurfaceSettings,
    /// Other terrain surfaces shown.
    pub other_surfaces: Vec<crate::surfaces::OtherSurface>,
    /// The surface the checks use, for everyone (a project setting;
    /// `None`: this user's ground).
    pub check_surface: Option<String>,
    /// That surface, read.
    pub check_ground: Option<std::sync::Arc<Terrain>>,
    /// The point cloud read (`FILE.ohl` or `FILE.ohl#name`).
    pub cloud: Option<String>,
    /// Orthophotos and drawings of the project (Terrain › Backgrounds).
    pub backgrounds: crate::backgrounds::Loaded,
    /// Backgrounds this user hides (`image:<layer>`, `drawing:<name>`).
    pub hidden_backgrounds: std::collections::BTreeSet<String>,
    /// The profile view's vertical exaggeration (heights drawn this many
    /// times larger than stations, kept when zooming); none: heights
    /// fitted to the stretch shown.
    pub profile_ratio: Option<f64>,
    /// The colours of the vegetation layers: grow-in points, danger trees
    /// (this user's view setting).
    pub vegetation_colors: [[u8; 3]; 2],
    /// Another user changed the drawing settings of the project line
    /// worked on (server mode): read them again.
    pub drawings_setting_changed: bool,
    /// The route worked on (View › Work on route): the profile, the
    /// checks and the loads keep to it, the rest is dimmed; none: all.
    pub work_route: Option<String>,
    /// LiDAR points shown (thinned to the budget).
    pub points: Vec<LidarPoint>,
    /// Origin of the points (and of the views), project coordinates.
    pub origin: [f64; 3],
    /// Class scheme of the points.
    pub scheme: ohl_lidar::ClassScheme,
    /// What was loaded, for the status panel.
    pub log: Vec<String>,
    /// Wire library (for re-solving after edits).
    pub library: ohl_cables::Library,
    /// Structure and insulator libraries, when the line uses types.
    pub types: Option<(
        ohl_structures::StructureLibrary,
        ohl_structures::InsulatorLibrary,
    )>,
    /// The types' bodies as drawn (levels 2 and 3), shared by their
    /// structures ([`drawn_bodies`]); made again with the libraries.
    pub bodies: std::sync::Arc<ohl_structures_fe::drawn::Bodies>,
    /// The project to edit, or why the model is read-only.
    pub editing: Result<Editing, String>,
    /// The project's session when it is read-only for this user (a
    /// viewer): kept for a joined project's replica.
    pub viewer_session: Option<ohl_session::Session>,
    /// The server, when the project is joined (server mode).
    pub remote: Option<crate::remote::Remote>,
    /// Weather cases being solved in the background: index, the line's
    /// version they started from, the answer.
    pub solving: Vec<(usize, u64, std::sync::mpsc::Receiver<Solved>)>,
    /// Counts the changes of `line` (a solve started before one is
    /// dropped).
    pub line_version: u64,
    /// Sections drawn in their own weather cases (indices; the first one
    /// is reported and picked), instead of the cases shown (Sections table).
    pub display_cases: std::collections::BTreeMap<String, Vec<usize>>,
    /// The weather cases shown for the whole line (Display weather cases;
    /// empty: the case passed to the views).
    pub shown_cases: Vec<usize>,
    /// The weather case of the copy clicked last, where several are drawn
    /// and it is not the one shown (highlighted in the views and the
    /// properties); `None`: the case shown.
    pub selection_case: Option<usize>,
    /// The structures coloured by usage in the plan and 3D views
    /// (Checks › Structure loads › Usage, Colour structures by usage); empty:
    /// not coloured.
    pub usage_marks: std::collections::BTreeMap<String, crate::loads::UsageMark>,
    /// The usage they show is out of date (the line changed since it was
    /// computed): the legend says so.
    pub usage_out_of_date: bool,
    /// Project lines shown next to the line worked on (Versions menu).
    pub other_lines: Vec<OtherLine>,
    /// Project lines another user changed since the last frame (server
    /// mode): those shown are read again; `""`: the line worked on was
    /// renamed or deleted (read again whole).
    pub lines_changed: Vec<String>,
    /// LiDAR classes not drawn (Terrain menu).
    pub hidden_classes: std::collections::BTreeSet<u8>,
    /// Points per class of the whole point cloud (loaded or not).
    pub class_counts: Vec<(u8, u64)>,
    /// Points of the cloud changed since the last frame (an edit, its
    /// undo, someone else's); the views read the points again
    /// ([`crate::lidar_edit::frame`]).
    pub points_changed: Option<crate::lidar_edit::PointsChanged>,
    /// The ground was built from the cloud's ground classes when the
    /// project opened (built again after a change of those points).
    pub ground_from_lidar: bool,
    /// The revisions of what results computed on demand are computed
    /// from ([`Model::revision`]): a result shows when it is out of date.
    pub revisions: crate::revision::Revisions,
    /// Share of the point cloud shown near the camera, % (level of detail
    /// 0; kept with the project, [`LIDAR_PERCENT_SETTING`]).
    pub points_percent: f64,
    /// Working folder of the project's libraries (the project holds them);
    /// `None`: read from their files (a line file, or read-only).
    pub library_dir: Option<PathBuf>,
    /// What is written next to the wires in each view (Sections › Display
    /// options; this user's choice).
    pub section_display: crate::annotations::SectionDisplay,
    /// The side the wind comes from in the weather cases shown (this
    /// user's choice).
    pub wind: WindDisplay,
    /// How the display weather cases apply to the sections (this user's
    /// choice).
    pub cases_shown: CasesShown,
    /// The table columns the plugins in use add (from their manifests).
    pub plugin_columns: Vec<crate::tables::PluginColumn>,
    /// The directories of the check plugins in use (extension point
    /// `check`).
    pub check_plugins: Vec<std::path::PathBuf>,
    /// The checks they run (their menus, titles, values), and the plugins
    /// that could not be read ([`Model::set_check_plugins`]).
    pub checks: Vec<ohl_io::checks::PluginCheck>,
    /// Why a check plugin could not be read.
    pub check_problems: Vec<String>,
}

/// The wire library: given, else the `*.ohlw` files next to the model,
/// else `libraries/cables`.
pub fn library_path(s: &Sources) -> PathBuf {
    if let Some(p) = &s.library {
        return p.clone();
    }
    let dir = dir_of(&s.file);
    let has_wires = std::fs::read_dir(&dir).is_ok_and(|mut it| {
        it.any(|e| {
            e.is_ok_and(|e| {
                ohl_core::files::has_extension(&e.path(), ohl_cables::WIRE_FILE_EXTENSION)
            })
        })
    });
    if has_wires {
        dir
    } else {
        shipped_library("cables")
    }
}

/// The line, and the project's session when it is a project file: the
/// project line `line` when it is one, else the one the file opens on.
fn load_line(
    path: &Path,
    line: Option<&str>,
) -> Result<(Line, Option<ohl_session::Session>), String> {
    if ohl_core::files::is_project_file(path) {
        let mut s = ohl_session::Session::open(path).map_err(|e| e.to_string())?;
        if let Some(l) = line.filter(|l| s.is_line(l)) {
            s.select_line(l).map_err(|e| e.to_string())?;
        }
        if !s.has_line() {
            return Err(format!("{}: the project holds no line", path.display()));
        }
        Ok((s.export(), Some(s)))
    } else {
        Ok((Line::load(path).map_err(|e| e.to_string())?, None))
    }
}

/// Who edits, and whether they may; the session when they may not.
fn editing(
    session: Option<ohl_session::Session>,
    user: Option<&str>,
) -> (Result<Editing, String>, Option<ohl_session::Session>) {
    let Some(session) = session else {
        return (
            Err("a line file: open a project (.ohl) to edit (ohl project import)".into()),
            None,
        );
    };
    let user = user.map(str::to_owned).unwrap_or_else(crate::files::user);
    match session.role(&user) {
        Err(e) => (Err(e.to_string()), Some(session)),
        Ok(Some(ohl_session::Role::Viewer)) => (
            Err(format!("user {user} is a viewer of this project")),
            Some(session),
        ),
        Ok(Some(_)) => (Ok(Editing { session, user }), None),
        Ok(None) => (
            Err(format!(
                "user {user} has no role in this project (an admin adds users: ohl project user)"
            )),
            Some(session),
        ),
    }
}

/// The structure types' directory: given, else `Structures/` next to the
/// model (types belong to a project: none are shipped).
pub fn structures_dir(sources: &Sources) -> PathBuf {
    sources
        .structures
        .clone()
        .unwrap_or_else(|| dir_of(&sources.file).join("Structures"))
}

/// Where a model's section library is read from when it has no copy of
/// its own: `Sections/` next to the model, else the folder of the model.
pub fn sections_dir(sources: &Sources) -> PathBuf {
    let dir = dir_of(&sources.file).join("Sections");
    if dir.is_dir() {
        dir
    } else {
        dir_of(&sources.file)
    }
}

/// The insulator assemblies' directory.
pub fn insulators_dir(sources: &Sources) -> PathBuf {
    sources
        .insulators
        .clone()
        .unwrap_or_else(|| shipped_library("insulators"))
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
        ohl_structures::StructureLibrary,
        ohl_structures::InsulatorLibrary,
    )>,
    String,
> {
    let dir = structures_dir(sources);
    if !(line.structures.iter().any(|s| s.structure_type.is_some()) || dir.is_dir()) {
        return Ok(None);
    }
    let structures = ohl_structures::StructureLibrary::load_dir(&dir).map_err(|e| e.to_string())?;
    let insulators = ohl_structures::InsulatorLibrary::load_dir(&insulators_dir(sources))
        .map_err(|e| e.to_string())?;
    Ok(Some((structures, insulators)))
}

/// The structure types' bodies as drawn in the 3D view and the drawings
/// (levels 2 and 3: members, their profiles from the section library in
/// `sections`; [`ohl_structures_fe::drawn`]): made once per type and
/// extension, shared by the structures of a type. A section library that
/// does not read leaves the members as lines.
///
/// Expected cost: reading each designed type's model (milliseconds).
pub fn drawn_bodies(
    types: Option<&(
        ohl_structures::StructureLibrary,
        ohl_structures::InsulatorLibrary,
    )>,
    sections: &Path,
) -> std::sync::Arc<ohl_structures_fe::drawn::Bodies> {
    let Some((structures, _)) = types else {
        return Default::default();
    };
    let library = ohl_sections::SectionLibrary::load(sections).unwrap_or_else(|e| {
        log::warn!("section library not read for the drawn members: {e}");
        Default::default()
    });
    std::sync::Arc::new(ohl_structures_fe::drawn::Bodies::load(structures, &library))
}

/// A LiDAR class of up to this many points is loaded whole (larger ones
/// are thinned to the point budget).
pub const SMALL_CLASS: u64 = 20_000;

/// A point cloud of more points is not read whole on opening: its ground
/// classes in the corridor, and its points shown from its levels of
/// detail (decoding every point takes ~1 s per 10⁷ points).
pub const SAMPLED_ABOVE: u64 = 20_000_000;

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

/// Project setting: the share of the point cloud shown near the camera, %.
pub const LIDAR_PERCENT_SETTING: &str = "app.lidar_percent";

/// Share of the point cloud shown near the camera by default, %.
pub const DEFAULT_LIDAR_PERCENT: f64 = 25.0;

/// Project setting holding the weather case file used.
pub const WEATHER_SETTING: &str = "app.weather";

/// Project setting: the project keeps libraries of its own from its
/// creation — nothing is copied in from files around it or given when it
/// opens (it starts empty; files come in from the library editors).
pub const OWN_LIBRARIES_SETTING: &str = "app.own-libraries";

/// Keeps the weather case file in the project's settings: its name when
/// it is one of the project's own files (in the libraries' working
/// folder), else its absolute path.
pub fn remember_weather(session: &ohl_session::Session, path: &Path) {
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
/// `loads`: the values typed over a structure's load table in the
/// structure application, `<stem>.loads.csv` (the type on its own) and
/// `<stem>.<placed structure>.loads.csv` (one placed structure), beside
/// the structure types ([`kind_files`]: only `*.loads.csv`).
pub const PROJECT_LIBRARIES: [(&str, &str); 13] = [
    ("cables", ohl_cables::WIRE_FILE_EXTENSION),
    (
        "structures",
        ohl_structures::structures::STRUCTURE_FILE_EXTENSION,
    ),
    (
        "insulators",
        ohl_structures::insulators::INSULATOR_FILE_EXTENSION,
    ),
    ("weather", ohl_weather::WEATHER_FILE_EXTENSION),
    ("loadcases", ohl_loads::LOAD_CASE_FILE_EXTENSION),
    ("rules", ohl_clearance::rules::RULE_FILE_EXTENSION),
    (
        "galloping",
        ohl_clearance::galloping::GALLOPING_FILE_EXTENSION,
    ),
    ("ambient", ohl_thermal::ambient::AMBIENT_FILE_EXTENSION),
    ("costs", ohl_bom::cost::COST_FILE_EXTENSION),
    ("poles", ohl_structures_fe::pole::POLE_FILE_EXTENSION),
    ("femodels", ohl_structures_fe::model::MODEL_FILE_EXTENSION),
    ("sections", ohl_sections::SECTION_FILE_EXTENSION),
    ("loads", "csv"),
];

/// The folder of library `kind` in the working folder and in an export:
/// poles, structure models and typed structure loads go next to the
/// structure types that name them.
pub fn library_folder(kind: &str) -> &str {
    match kind {
        "poles" | "femodels" | "loads" => "structures",
        _ => kind,
    }
}

/// The files of library `kind` (extension `extension`) at `at`: for
/// `loads` only the `*.loads.csv` files.
fn kind_files(at: &Path, kind: &str, extension: &str) -> Vec<PathBuf> {
    let mut files = files_at(at, extension);
    if kind == "loads" {
        files.retain(|f| {
            f.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".loads.csv"))
        });
    }
    files
}

/// Where the working copies of projects' libraries go.
fn libraries_root() -> PathBuf {
    let t = std::env::temp_dir();
    std::fs::canonicalize(&t)
        .unwrap_or(t)
        .join("ohl-design-libraries")
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
    files_with_extension(at, extension)
}

/// The project's libraries: each kind the project does not hold yet is
/// copied into it from where the model would read it (given, next to the
/// project, or the default folders); one given on the command line
/// replaces the project's. Then they are written to a working folder and
/// `sources` points there. Without the right to change the project, the
/// libraries it lacks are read from their files as before.
/// A weather case file's text without the wind side of its cases
/// (`wind_from`, a display choice since), `None` when it has none.
pub fn without_wind_side(text: &str) -> Option<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().ok()?;
    let mut changed = false;
    if let Some(cases) = doc.get_mut("case").and_then(|c| c.as_array_of_tables_mut()) {
        for case in cases.iter_mut() {
            changed |= case.remove("wind_from").is_some();
        }
    }
    changed.then(|| doc.to_string())
}

/// The project line's libraries as working copies (files the readers
/// take, `sources` pointed at them); `import`: when the project opens,
/// files given or next to it copied into the project first.
fn project_libraries(
    sources: &mut Sources,
    session: &mut ohl_session::Session,
    import: bool,
    log: &mut dyn FnMut(String),
) -> Result<PathBuf, String> {
    let user = sources.user.clone().unwrap_or_else(crate::files::user);
    // The weather file used last, when none is given and it is a file.
    let remembered = session.setting(WEATHER_SETTING).ok().flatten();
    let own = session
        .setting(OWN_LIBRARIES_SETTING)
        .ok()
        .flatten()
        .is_some();
    let work = library_workdir(&sources.file);
    let _ = std::fs::remove_dir_all(&work);
    // Poles and structure models: from the structure types' folder (before
    // it points to the working copies).
    let (structures_given, structures_from) =
        (sources.structures.is_some(), structures_dir(sources));
    for (kind, ext) in PROJECT_LIBRARIES {
        let given = match kind {
            "cables" => sources.library.is_some(),
            "structures" => sources.structures.is_some(),
            "insulators" => sources.insulators.is_some(),
            "loadcases" => sources.load_cases.is_some(),
            "weather" => sources.weather.is_some(),
            "poles" | "femodels" | "loads" => structures_given,
            _ => false,
        };
        let stored = session.library_files(kind).map_err(|e| e.to_string())?;
        let mut kept_in_files = false;
        if import && !own && (stored.is_empty() || given) {
            // Files given, or next to the project: never the libraries
            // shipped with the program (a new project starts empty).
            let own =
                |p: PathBuf, shipped: &str| (given || p != shipped_library(shipped)).then_some(p);
            let from: Option<PathBuf> = match kind {
                "cables" => own(library_path(sources), "cables"),
                "structures" => own(structures_dir(sources), "structures"),
                "insulators" => own(insulators_dir(sources), "insulators"),
                "loadcases" => sources.load_cases.clone().or_else(|| {
                    files_at(&model_dir(sources), ohl_loads::LOAD_CASE_FILE_EXTENSION)
                        .into_iter()
                        .next()
                }),
                "weather" => sources.weather.clone().or_else(|| {
                    remembered
                        .as_deref()
                        .map(PathBuf::from)
                        .filter(|p| p.is_absolute() && p.is_file())
                }),
                "poles" | "femodels" | "loads" => own(structures_from.clone(), "structures"),
                // The `Sections/` folder next to the project, else the
                // files next to it.
                "sections" => own(sections_dir(sources), "sections"),
                // Clearance rules, galloping, ambient cases, unit costs:
                // the files next to the project (none are shipped).
                _ => Some(model_dir(sources)),
            };
            let files = from
                .as_deref()
                .map(|f| kind_files(f, kind, ext))
                .unwrap_or_default();
            if !files.is_empty() {
                // One log entry, not undone by Undo (the program's doing).
                let mut copy = || -> Result<(), String> {
                    let mut changes = Vec::new();
                    // Given: replaces the project's (weather: adds a file).
                    if given && kind != "weather" && kind != "loadcases" {
                        for (name, _) in &stored {
                            changes.push((kind.to_owned(), name.clone(), None));
                        }
                    }
                    for f in &files {
                        let text = std::fs::read_to_string(f)
                            .map_err(|e| format!("{}: {e}", f.display()))?;
                        let name = f.file_name().map(|n| n.to_string_lossy().into_owned());
                        let name = name.unwrap_or_default();
                        changes.retain(|(_, n, _)| *n != name);
                        changes.push((kind.to_owned(), name, Some(text)));
                    }
                    session
                        .keep_library_files(&user, &changes)
                        .map_err(|e| e.to_string())
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
        let mut stored = session.library_files(kind).map_err(|e| e.to_string())?;
        if stored.is_empty() {
            // Its own empty library (saved files go into the project).
            if matches!(kind, "cables" | "structures" | "insulators") {
                let dir = work.join(kind);
                std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                match kind {
                    "cables" => sources.library = Some(dir),
                    "structures" => sources.structures = Some(dir),
                    _ => sources.insulators = Some(dir),
                }
            } else if matches!(kind, "weather" | "loadcases") {
                if own {
                    // None of its own yet: not those of another project.
                    match kind {
                        "weather" => sources.weather = None,
                        _ => sources.load_cases = None,
                    }
                }
            } else {
                // Its own empty folder: none read from next to the project.
                let dir = work.join(library_folder(kind));
                std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            continue;
        }
        // Weather case files of earlier versions gave the wind side: now a
        // display choice. Converted when the project opens (kept as an
        // edit of the library file when this user may edit).
        if kind == "weather" {
            for (name, text) in &mut stored {
                if let Some(converted) = without_wind_side(text) {
                    match session.keep_library_files(
                        &user,
                        &[(kind.to_owned(), name.clone(), Some(converted.clone()))],
                    ) {
                        Ok(()) => log(format!(
                            "weather cases {name}: the wind side removed (now chosen in \
                             Display weather cases)"
                        )),
                        Err(e) => log(format!("weather cases {name}: not converted ({e})")),
                    }
                    *text = converted;
                }
            }
        }
        let dir = work.join(library_folder(kind));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (name, text) in &stored {
            if !ohl_session::safe_file_name(name) {
                return Err(format!("library file \"{name}\": not a plain file name"));
            }
            std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"))?;
        }
        match kind {
            "cables" => sources.library = Some(dir),
            "structures" => sources.structures = Some(dir),
            "insulators" => sources.insulators = Some(dir),
            "loadcases" => {
                let wanted = sources
                    .load_cases
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                let name = wanted
                    .filter(|n| stored.iter().any(|(s, _)| s == n))
                    .unwrap_or_else(|| stored[0].0.clone());
                sources.load_cases = Some(dir.join(name));
            }
            "weather" => {
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
            // Read from the working folder ([`Model::project_files`]).
            _ => {}
        }
        log(format!("{kind}: {} file(s) from the project", stored.len()));
    }
    Ok(work)
}

/// What the route worked on holds (View › Work on route).
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Structures on it.
    pub structures: std::collections::BTreeSet<String>,
    /// Sections on it.
    pub sections: std::collections::BTreeSet<String>,
    /// Jumpers on it.
    pub jumpers: std::collections::BTreeSet<String>,
}

impl Model {
    /// What the route worked on holds; none: all the line is worked on.
    pub fn scope(&self) -> Option<Scope> {
        let r = self.work_route.as_ref()?;
        let on = |rs: &Vec<String>| rs.contains(r);
        let (sections, jumpers) = self.line.wire_routes();
        Some(Scope {
            structures: self
                .line
                .structure_routes()
                .into_iter()
                .filter(|(_, rs)| on(rs))
                .map(|(id, _)| id)
                .collect(),
            sections: sections
                .into_iter()
                .filter(|(_, rs)| on(rs))
                .map(|(id, _)| id)
                .collect(),
            jumpers: jumpers
                .into_iter()
                .filter(|(_, rs)| on(rs))
                .map(|(id, _)| id)
                .collect(),
        })
    }

    /// The files of library `kind` (clearance rules, galloping, ambient
    /// cases, unit costs): the project's working copies, else (a line
    /// file, or kept in their files) those next to the model.
    pub fn project_files(&self, kind: &str) -> Vec<PathBuf> {
        let ext = PROJECT_LIBRARIES
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or("", |(_, e)| e);
        let dir = self
            .library_dir
            .as_ref()
            .map(|w| w.join(library_folder(kind)))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| model_dir(&self.sources));
        files_with_extension(&dir, ext)
    }

    /// The section library (`.ohlsec`) the structure models refer to: the
    /// project's working copies, else ([`sections_dir`]) the files next to
    /// the model; empty when there are none.
    pub fn section_library(&self) -> Result<ohl_sections::SectionLibrary, String> {
        ohl_sections::SectionLibrary::load(&self.sections_folder())
    }

    /// Where the section library is read from: the project's working
    /// copies, else [`sections_dir`].
    pub fn sections_folder(&self) -> PathBuf {
        self.library_dir
            .as_ref()
            .map(|w| w.join(library_folder("sections")))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| sections_dir(&self.sources))
    }

    /// Keeps the working copies of the project's libraries in the project
    /// (after a library page or the weather case table saved one): every
    /// file of the working folder put, files no longer there removed.
    /// Nothing for a line file or libraries read from their files.
    pub fn store_libraries(&mut self) -> Result<usize, String> {
        let (Some(work), Ok(e)) = (&self.library_dir, &self.editing) else {
            return Ok(0);
        };
        // What changed, as project edits (sent to the server when joined).
        let mut ops = Vec::new();
        let mut n = 0;
        for (kind, ext) in PROJECT_LIBRARIES {
            let dir = work.join(library_folder(kind));
            if !dir.is_dir() {
                continue;
            }
            let stored: std::collections::BTreeMap<String, String> = e
                .session
                .library_files(kind)
                .map_err(|e| e.to_string())?
                .into_iter()
                .collect();
            let files = kind_files(&dir, kind, ext);
            let names: Vec<String> = files
                .iter()
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            for (f, name) in files.iter().zip(&names) {
                let text = std::fs::read_to_string(f).map_err(|e| format!("{name}: {e}"))?;
                n += 1;
                if stored.get(name) != Some(&text) {
                    ops.push(ohl_server::Op::PutLibrary {
                        kind: kind.to_owned(),
                        name: name.clone(),
                        text,
                    });
                }
            }
            for name in stored.keys() {
                if !names.contains(name) {
                    ops.push(ohl_server::Op::DeleteLibrary {
                        kind: kind.to_owned(),
                        name: name.clone(),
                    });
                }
            }
        }
        for op in ops {
            let what = op.describe();
            self.project_op(&what, op).map_err(|e| e.to_string())?;
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
            let to = dir.join(library_folder(kind));
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            for (name, text) in files {
                // A name from the project file (a crafted one: "../…", an
                // absolute path) never leaves the folder chosen.
                if !ohl_session::safe_file_name(&name) {
                    return Err(format!("library file \"{name}\": not a plain file name"));
                }
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
            ohl_core::units::Temperature::from_celsius(15.0),
        )],
    })
}

impl Model {
    /// The ground the checks use: the project's surface for them, else
    /// this user's ground.
    pub fn ground_for_checks(&self) -> Option<std::sync::Arc<Terrain>> {
        match &self.check_surface {
            Some(n) if self.surface_settings.active.as_ref() == Some(n) => self.ground.clone(),
            Some(_) => self.check_ground.clone(),
            None => self.ground.clone(),
        }
    }

    /// Reads the surface of the checks when it is not read yet (a check
    /// needs it).
    pub fn ensure_check_ground(&mut self) {
        if let Some(n) = &self.check_surface
            && self.surface_settings.active.as_ref() != Some(n)
            && self.check_ground.is_none()
        {
            match crate::surfaces::load(&self.sources.file, n) {
                Ok(g) => self.check_ground = Some(std::sync::Arc::new(g)),
                Err(e) => log::warn!("surface of the checks {n}: {e}"),
            }
        }
    }

    /// Uses the class scheme file `path` (`None`: the point cloud's own)
    /// for this project: names, roles and colours of the LiDAR classes.
    pub fn use_class_scheme(&mut self, path: Option<PathBuf>) -> Result<(), String> {
        self.scheme = match &path {
            Some(p) => ohl_lidar::ClassScheme::load(p)?,
            None => {
                let own = self
                    .cloud
                    .as_ref()
                    .and_then(|spec| ohl_pointcloud::open_source(spec).ok()?.class_scheme());
                match own {
                    Some(text) => ohl_lidar::ClassScheme::parse(&text)?,
                    None => ohl_lidar::ClassScheme::asprs(),
                }
            }
        };
        // Remembered: an empty path is "the cloud's own".
        self.sources.class_scheme = Some(path.unwrap_or_default());
        Ok(())
    }

    /// What this user shows of the project now (remembered locally).
    pub fn view_prefs(&self) -> ViewPrefs {
        ViewPrefs {
            surfaces: self.surface_settings.clone(),
            lidar_percent: Some(self.points_percent),
            hidden_classes: self.hidden_classes.clone(),
            class_scheme: self.sources.class_scheme.clone(),
            section_display: self.section_display.clone(),
            wind: self.wind,
            cases_shown: self.cases_shown,
            hidden_backgrounds: self.hidden_backgrounds.clone(),
            profile_ratio: self.profile_ratio,
            work_route: self.work_route.clone(),
            vegetation_colors: Some(self.vegetation_colors),
            line: self.session().map(|s| s.current_line().to_owned()),
            shown_lines: self.other_lines.iter().map(|o| o.name.clone()).collect(),
        }
    }

    /// Solves the weather cases `wanted` that are not solved yet, in the
    /// background (one thread each); [`Model::poll_solving`] takes them.
    pub fn want(&mut self, wanted: impl IntoIterator<Item = usize>, ctx: &eframe::egui::Context) {
        for k in wanted {
            let Some(c) = self.cases.get(k) else {
                continue;
            };
            if c.solved_for(self.wind) || self.solving.iter().any(|(j, _, _)| *j == k) {
                continue;
            }
            let (line, library, case) = (self.line.clone(), self.library.clone(), c.case.clone());
            let wind = self.wind;
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Solved::with_wind(&line, &library, case, wind));
                ctx.request_repaint();
            });
            self.solving.push((k, self.line_version, rx));
        }
    }

    /// Takes the weather cases solved in the background; their names. A
    /// case solved from a line changed since is dropped (wanted again, it
    /// is solved again).
    pub fn poll_solving(&mut self) -> Vec<String> {
        let mut done = Vec::new();
        let version = self.line_version;
        let mut keep = Vec::new();
        for (k, v, rx) in std::mem::take(&mut self.solving) {
            match rx.try_recv() {
                Ok(mut s) if v == version && k < self.cases.len() => {
                    done.push(s.case.name.clone());
                    s.inherit(&self.cases[k]);
                    self.cases[k] = s;
                }
                Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => keep.push((k, v, rx)),
            }
        }
        self.solving = keep;
        done
    }

    /// Solves weather case `k` now if it is not (a check or a script
    /// needs it).
    pub fn ensure_solved(&mut self, k: usize) {
        if let Some(c) = self.cases.get(k)
            && !c.solved_for(self.wind)
        {
            let mut s = Solved::with_wind(&self.line, &self.library, c.case.clone(), self.wind);
            s.inherit(c);
            self.solving.retain(|(j, _, _)| *j != k);
            self.cases[k] = s;
        }
    }

    /// The weather cases section `section` is drawn in: its own display
    /// cases, else the cases shown for the line, else `case`.
    pub fn section_cases(&self, section: &str, case: usize) -> Vec<usize> {
        let valid = |v: &Vec<usize>| -> Vec<usize> {
            v.iter()
                .copied()
                .filter(|k| *k < self.cases.len())
                .collect()
        };
        let own = match self.cases_shown {
            CasesShown::AllSections => Vec::new(),
            CasesShown::PerSection => self
                .display_cases
                .get(section)
                .map(valid)
                .unwrap_or_default(),
        };
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

    /// The check plugins in use are those in `dirs`: their checks read
    /// (each plugin compiled once, a fraction of a second), when they
    /// changed.
    pub fn set_check_plugins(&mut self, dirs: Vec<std::path::PathBuf>) {
        if self.check_plugins == dirs {
            return;
        }
        (self.checks, self.check_problems) = if dirs.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            match ohl_plugins::WasmHost::new() {
                Ok(host) => ohl_io::checks::list(&host, &dirs),
                Err(e) => (Vec::new(), vec![e.to_string()]),
            }
        };
        self.check_plugins = dirs;
    }

    /// Kind of the project's library files of point and spread load
    /// types (`.ohlload`).
    pub const OBJECT_LIBRARY_KIND: &'static str = "objects";

    /// The point and spread load types to choose from: the shipped
    /// templates, then the project's own (an own type of a template's id
    /// replaces it).
    pub fn object_library(&self) -> ohl_line::ObjectLibrary {
        let mut lib = ohl_line::ObjectLibrary::templates();
        if let Ok(e) = &self.editing
            && let Ok(files) = e.session.library_files(Self::OBJECT_LIBRARY_KIND)
        {
            for (_, text) in files {
                if let Ok(own) = ohl_line::ObjectLibrary::parse(&text) {
                    for t in own.types {
                        lib.types.retain(|x| x.id != t.id);
                        lib.types.push(t);
                    }
                }
            }
        }
        lib
    }

    /// A jumper as solved in weather case `case`.
    pub fn jumper_shape(&self, id: &str, case: usize) -> Option<&ohl_line::jumper::JumperShape> {
        self.cases
            .get(self.shown(case).first().copied().unwrap_or(case))?
            .jumpers
            .iter()
            .find(|j| j.jumper == id)
    }

    /// Section `section` not solved in weather case `k` (the case's
    /// primary side, or its mirror `side` 1), drawn where it was: its
    /// spans' points, where they come from, why it did not solve. The
    /// shape is its last solved one in this case ([`Solved::last_solved`]),
    /// else its shape in another weather case solved, else the straight
    /// chords between its wire points. None: solved (or not a section).
    pub fn unsolved_drawn(&self, section: &str, k: usize, side: usize) -> Option<UnsolvedDrawn> {
        let solved = self.cases.get(k)?.drawn().nth(side)?;
        let reason = solved
            .unsolved
            .iter()
            .find(|(id, _)| id == section)?
            .1
            .clone();
        let spans = |shape: &SectionShape| -> Vec<Vec<[f64; 3]>> {
            shape.spans.iter().map(|s| s.points.clone()).collect()
        };
        if let Some(shape) = solved.last_solved.get(section) {
            return Some(UnsolvedDrawn {
                spans: spans(shape),
                from: "its last solved shape in this case".into(),
                reason,
            });
        }
        for (_, c) in self.cases.iter().enumerate().filter(|(j, _)| *j != k) {
            if let Some(shape) = c
                .shapes
                .iter()
                .chain(c.last_solved.values())
                .find(|s| s.section == section)
            {
                return Some(UnsolvedDrawn {
                    spans: spans(shape),
                    from: format!("its shape in weather case {}", c.case.name),
                    reason,
                });
            }
        }
        let sec = self.line.sections.iter().find(|s| s.id == section)?;
        let at: Vec<Option<[f64; 3]>> = sec
            .points
            .iter()
            .map(|p| self.line.wire_point(p).map(|w| w.position))
            .collect();
        let chords = at
            .windows(2)
            .map(|w| match (w[0], w[1]) {
                (Some(a), Some(b)) => vec![a, b],
                _ => Vec::new(),
            })
            .collect();
        Some(UnsolvedDrawn {
            spans: chords,
            from: "the straight chords between its wire points (never solved)".into(),
            reason,
        })
    }

    /// Section `section` as shown (its display case, else `case`).
    pub fn shape(&self, section: &str, case: usize) -> Option<&SectionShape> {
        self.cases
            .get(self.section_case(section, case))?
            .shapes
            .iter()
            .find(|s| s.section == section)
    }

    /// The wind side shown: every case's primary side set, mirrors dropped
    /// or wanted ([`Model::want`] solves what is shown again).
    pub fn set_wind(&mut self, wind: WindDisplay) {
        self.wind = wind;
        for c in &mut self.cases {
            if c.case.wind_pressure.si() > 0.0 && c.case.wind_from != wind.primary() {
                let mut case = c.case.clone();
                case.wind_from = wind.primary();
                *c = Solved::pending(case);
            }
            if wind != WindDisplay::Both {
                c.mirror = None;
            }
        }
        // Answers on their way were solved for the old side.
        self.solving.clear();
        self.line_version += 1;
    }

    /// Every section in each weather case it is drawn in, in line order
    /// (both wind sides when shown).
    pub fn shapes_shown(&self, case: usize) -> Vec<SectionShape> {
        self.shapes_shown_ref(case).into_iter().cloned().collect()
    }

    /// [`Model::shapes_shown`] without copying them (drawn every frame).
    pub fn shapes_shown_ref(&self, case: usize) -> Vec<&SectionShape> {
        self.line
            .sections
            .iter()
            .flat_map(|s| {
                self.section_cases(&s.id, case)
                    .into_iter()
                    .filter_map(|k| self.cases.get(k))
                    .flat_map(|c| c.drawn())
                    .filter_map(|c| c.shapes.iter().find(|x| x.section == s.id))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Every section not solved in a weather case it is drawn in (both
    /// wind sides), drawn in its place ([`Model::unsolved_drawn`]): the
    /// section, the case, the drawing.
    pub fn unsolved_shown(&self, case: usize) -> Vec<(String, usize, UnsolvedDrawn)> {
        let mut out = Vec::new();
        for s in &self.line.sections {
            for k in self.section_cases(&s.id, case) {
                let Some(c) = self.cases.get(k) else {
                    continue;
                };
                for side in 0..c.drawn().count() {
                    if let Some(d) = self.unsolved_drawn(&s.id, k, side) {
                        out.push((s.id.clone(), k, d));
                    }
                }
            }
        }
        out
    }

    /// The project's session (edited, or read only for a viewer).
    pub fn session(&self) -> Option<&ohl_session::Session> {
        match &self.editing {
            Ok(e) => Some(&e.session),
            Err(_) => self.viewer_session.as_ref(),
        }
    }

    /// The user's name: in the project edited, on the server joined, else
    /// the login name.
    pub fn user_name(&self) -> String {
        if let Some(r) = &self.remote {
            return r.link.user.clone();
        }
        match &self.editing {
            Ok(e) => e.user.clone(),
            Err(_) => self.sources.user.clone().unwrap_or_else(crate::files::user),
        }
    }

    /// The project line worked on (empty: a line file, no project).
    pub fn project_line(&self) -> String {
        self.session()
            .map(|s| s.current_line().to_owned())
            .unwrap_or_default()
    }

    /// Works on project line `name` from now on (this user only; nothing
    /// changes for the others): the line is read, resolved and solved
    /// again; shown next to it, the line left.
    pub fn switch_line(&mut self, name: &str) -> Result<String, String> {
        let from = self.project_line();
        if from == name {
            return Ok(String::new());
        }
        let session = match &mut self.editing {
            Ok(e) => &mut e.session,
            Err(why) => self.viewer_session.as_mut().ok_or_else(|| why.clone())?,
        };
        // Each project line has its library files: read again when they
        // differ.
        let files = |s: &ohl_session::Session| {
            PROJECT_LIBRARIES
                .iter()
                .map(|(k, _)| s.library_files(k).unwrap_or_default())
                .collect::<Vec<_>>()
        };
        let before = files(session);
        session.select_line(name).map_err(|e| e.to_string())?;
        let libraries_differ = files(session) != before;
        if let Some(r) = &mut self.remote {
            r.link.line = name.to_owned();
        }
        self.other_lines.retain(|o| o.name != name);
        let started = Instant::now();
        if libraries_differ {
            self.refresh_project_libraries()?;
        } else {
            self.reread_line()?;
        }
        Ok(format!(
            "working on project line {name} now (read in {:.0?}; weather cases solving)",
            started.elapsed()
        ))
    }

    /// Reads the session's line again (another project line chosen, or
    /// renamed or deleted by someone else): its structure types resolved
    /// with the libraries in use (those of a line chosen with other
    /// library files are read again first, [`Model::switch_line`]) and
    /// its weather cases solved in the background, those shown first (as
    /// when the project opens). The project lines shown next to it are
    /// kept as solved. Cost: resolving the line (milliseconds); the views
    /// show it at once, the wires as each case is solved.
    pub fn reread_line(&mut self) -> Result<(), String> {
        let stored = self.session().map(|s| s.export()).ok_or("no project")?;
        let mut line = match &self.types {
            Some((s, i)) => stored.resolve(s, i).map_err(|e| e.to_string())?,
            None => stored,
        };
        let cases: Vec<WeatherCase> = self.cases.iter().map(|c| c.case.clone()).collect();
        line.set_weather_cases(&cases);
        // Solutions of the line left are dropped, those under way too.
        self.solving.clear();
        self.line_version += 1;
        self.cases = cases.into_iter().map(Solved::pending).collect();
        self.line = line;
        Ok(())
    }

    /// Reads project lines `names` again where they are shown (another
    /// user changed them).
    pub fn refresh_lines(&mut self, names: &[String]) {
        for name in names {
            let Some(k) = self.other_lines.iter().position(|o| &o.name == name) else {
                continue;
            };
            let color = self.other_lines.remove(k).color;
            match self.show_line(name) {
                Ok(_) => {
                    if let Some(o) = self.other_lines.last_mut() {
                        o.color = color;
                    }
                    let last = self.other_lines.pop().expect("just shown");
                    self.other_lines.insert(k.min(self.other_lines.len()), last);
                }
                Err(e) => log::warn!("project line {name}: {e}"),
            }
        }
    }

    /// Shows project line `name` next to the line worked on (solved in
    /// every weather case).
    pub fn show_line(&mut self, name: &str) -> Result<String, String> {
        if self.other_lines.iter().any(|o| o.name == name) {
            return Ok(String::new());
        }
        let started = Instant::now();
        let session = self.session().ok_or("open a project to show its lines")?;
        let stored = session.line_named(name).map_err(|e| e.to_string())?.clone();
        let mut line = match &self.types {
            Some((s, i)) => stored.resolve(s, i).map_err(|e| e.to_string())?,
            None => stored,
        };
        let weather: Vec<WeatherCase> = self.cases.iter().map(|c| c.case.clone()).collect();
        line.set_weather_cases(&weather);
        let cases: Vec<Solved> = self
            .cases
            .iter()
            .map(|c| Solved::new(&line, &self.library, c.case.clone()))
            .collect();
        // A colour no line shown has yet.
        let color = OTHER_COLORS
            .iter()
            .copied()
            .find(|c| self.other_lines.iter().all(|o| o.color != *c))
            .unwrap_or(OTHER_COLORS[self.other_lines.len() % OTHER_COLORS.len()]);
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

    /// The project line's library files written again as working copies
    /// and read again (another project line chosen, a library change
    /// undone or redone, a merge).
    pub fn refresh_project_libraries(&mut self) -> Result<String, String> {
        let session = match &mut self.editing {
            Ok(e) => &mut e.session,
            Err(why) => self.viewer_session.as_mut().ok_or_else(|| why.clone())?,
        };
        project_libraries(&mut self.sources, session, false, &mut |_| {})?;
        self.reload_libraries()
    }

    /// Reads the libraries again (wires, structure types, insulators,
    /// weather cases) after one was edited, resolves the line again and
    /// solves every case. Nothing changes when one does not load.
    pub fn reload_libraries(&mut self) -> Result<String, String> {
        let started = Instant::now();
        let stored = match self.session() {
            Some(s) => s.export(),
            None => load_line(&self.sources.file, None)?.0,
        };
        let types = load_types(&self.sources, &stored)?;
        let mut line = match &types {
            Some((s, i)) => stored.resolve(s, i).map_err(|e| e.to_string())?,
            None => stored,
        };
        let library =
            ohl_cables::Library::load(&library_path(&self.sources)).map_err(|e| e.to_string())?;
        let cases = load_cases(&self.sources)?;
        line.set_weather_cases(&cases);
        // The cases solved before (by name, and the first) again; the
        // others when shown.
        let was: Vec<String> = self
            .cases
            .iter()
            .filter(|c| c.ready)
            .map(|c| c.case.name.clone())
            .collect();
        self.solving.clear();
        self.line_version += 1;
        self.revisions.libraries += 1;
        let old = std::mem::take(&mut self.cases);
        self.cases = cases
            .into_iter()
            .enumerate()
            .map(|(k, mut c)| {
                c.wind_from = self.wind.primary();
                if k == 0 || was.contains(&c.name) {
                    let mut s = Solved::with_wind(&line, &library, c, self.wind);
                    // Sections that stop solving keep their last shape.
                    if let Some(o) = old.iter().find(|o| o.case.name == s.case.name) {
                        s.inherit(o);
                    }
                    s
                } else {
                    Solved::pending(c)
                }
            })
            .collect();
        // Project lines shown: solved again in the new cases.
        let others: Vec<String> = self.other_lines.drain(..).map(|o| o.name).collect();
        self.line = line;
        self.library = library;
        self.bodies = drawn_bodies(types.as_ref(), &self.sections_folder());
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
    let wanted = sources.view.as_ref().and_then(|v| v.line.clone());
    let (mut line, mut session) = load_line(&sources.file, wanted.as_deref())?;
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
    let library_dir = match &mut session {
        Some(s) => Some(project_libraries(&mut sources, s, true, &mut |m| {
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
                Some(w) if dir.starts_with(w) => {
                    "the structure library from the project".to_owned()
                }
                _ => format!("the structure library from {}", dir.display()),
            },
            &mut log,
        );
    }
    let lib_path = library_path(&sources);
    let library = ohl_cables::Library::load(&lib_path).map_err(|e| e.to_string())?;
    if let Some(w) = ohl_cables::mechanics::curve_offset_warning(&library) {
        log.push(format!("wire library: {w}"));
    }
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
    // The creep-stretch weather cases, for sections after creep or load.
    line.set_weather_cases(&cases);
    let missing = line.missing_condition_cases();
    if !missing.is_empty() {
        step(
            format!(
                "creep-stretch weather cases not in the weather case file: {}",
                missing.join(", ")
            ),
            &mut log,
        );
    }
    // Only the weather case shown first is solved now; the others when
    // they are shown or a check needs them (Model::want).
    let mut solved = Vec::new();
    let others = cases.len().saturating_sub(1);
    // The wind side is this user's display choice, not the file's.
    let wind = sources
        .view
        .as_ref()
        .map_or_else(WindDisplay::default, |v| v.wind);
    for (k, mut case) in cases.into_iter().enumerate() {
        case.wind_from = wind.primary();
        if k > 0 {
            solved.push(Solved::pending(case));
            continue;
        }
        let t = Instant::now();
        let name = case.name.clone();
        let one = Solved::with_wind(&line, &library, case, wind);
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
    if others > 0 {
        step(
            format!("{others} other weather case(s): solved when shown"),
            &mut log,
        );
    }
    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let mut ground_source = String::new();
    let mut ground = match &sources.terrain {
        Some(p) => {
            let t = Terrain::load(p).map_err(|e| e.to_string())?;
            step(format!("ground: {} triangles", t.num_triangles()), &mut log);
            ground_source = format!("survey file {}", p.display());
            Some(t)
        }
        None => None,
    };
    // The project's terrain surfaces: the one this user uses is the ground
    // (no ground built from LiDAR then); those shown are read too. This
    // user's choice, else (projects from earlier builds) the project's.
    let surface_settings = match &sources.view {
        Some(v) => v.surfaces.clone(),
        None => session
            .as_ref()
            .map(crate::surfaces::SurfaceSettings::read)
            .unwrap_or_default(),
    };
    let mut other_surfaces = Vec::new();
    if session.is_some() {
        if ground.is_none()
            && let Some(name) = &surface_settings.active
        {
            let t = Instant::now();
            match crate::surfaces::load(&sources.file, name) {
                Ok(g) => {
                    step(
                        format!(
                            "ground: surface {name}, {} triangles, read in {:.1?}",
                            g.num_triangles(),
                            t.elapsed()
                        ),
                        &mut log,
                    );
                    ground_source = format!("surface {name}");
                    ground = Some(g);
                }
                Err(e) => step(format!("surface {name} not read: {e}"), &mut log),
            }
        }
        for (name, color) in &surface_settings.shown {
            match crate::surfaces::load(&sources.file, name) {
                Ok(g) => other_surfaces.push(crate::surfaces::OtherSurface {
                    name: name.clone(),
                    terrain: std::sync::Arc::new(g),
                    color: *color,
                }),
                Err(e) => step(format!("surface {name} not read: {e}"), &mut log),
            }
        }
    }
    // Point cloud: given, else the project's own.
    let cloud = sources.pointcloud.clone().or_else(|| {
        let is_project = ohl_core::files::is_project_file(&sources.file);
        let has = is_project
            && ohl_pointcloud::Store::open(&sources.file)
                .and_then(|s| s.clouds())
                .is_ok_and(|c| !c.is_empty());
        has.then(|| sources.file.display().to_string())
    });
    let mut points = Vec::new();
    let mut ground_from_lidar = false;
    let mut class_counts: Vec<(u8, u64)> = Vec::new();
    let mut scheme = ohl_lidar::ClassScheme::asprs();
    if let Some(spec) = cloud.clone() {
        let t = Instant::now();
        // A project's cloud or a COPC file.
        let source = ohl_pointcloud::open_source(&spec)?;
        if let Some(text) = source.class_scheme() {
            scheme = ohl_lidar::ClassScheme::parse(&text)?;
        }
        // This user's scheme (names, roles, colours), if chosen (an empty
        // path: the cloud's own).
        if let Some(p) = sources
            .class_scheme
            .as_ref()
            .filter(|p| !p.as_os_str().is_empty())
        {
            match ohl_lidar::ClassScheme::load(p) {
                Ok(s) => {
                    step(
                        format!("class scheme {}: {}", s.name, p.display()),
                        &mut log,
                    );
                    scheme = s;
                }
                Err(e) => step(format!("class scheme not read: {e}"), &mut log),
            }
        }
        let ground_classes = scheme.codes_with_role("ground");
        let mut is_ground = [false; 256];
        for &c in &ground_classes {
            is_ground[c as usize] = true;
        }
        // Corridor along the spans' chords.
        let corridor = ohl_lidar::Corridor::along_spans(&line, sources.corridor);
        // Every class shown, of the whole cloud: a class of up to
        // SMALL_CLASS points whole, the others every n-th point, n from
        // the cloud's size. (Thinning every class alike and leaving out the
        // ground classes, as before, lost whole classes of a real survey:
        // roads, water, bridges, and classes of 6 to 500 points.)
        let total = source.points().unwrap_or(0);
        let stride = (total as usize / sources.point_budget.max(1)).max(1);
        let known = source.class_counts();
        class_counts = known.clone().unwrap_or_default();
        let mut thinning = Thinning::new(&class_counts, stride);
        // The ground as the surface window builds it from LiDAR.
        let build = crate::surfaces::LidarBuild::default();
        let mut ground_points = ohl_lidar::LowestPerCell::new(Some(build.cell));
        let mm = |v: f64, i: usize| ((v - origin[i]) * 1000.0).round() as i32;
        // A surface or survey file gives the ground: no LiDAR ground then.
        let need_ground = ground.is_none();
        // Exit clause: decoding is ~1 s per 10⁷ points; 10 min is far beyond.
        let budget =
            ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
        let stats = if total <= SAMPLED_ABOVE && known.is_some() {
            // One pass over every point.
            source.visit(None, None, &budget, &mut |x, y, z, c| {
                if thinning.keep(c) {
                    points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c));
                }
                // The ground surface: ground classes in the corridor.
                if need_ground && is_ground[c as usize] && corridor.contains(x, y) {
                    ground_points.add(x, y, z);
                }
            })?
        } else {
            // A large cloud (or a COPC file): the ground classes in the
            // corridor, then the points shown from the levels of detail —
            // a small part of the data, not every point.
            let mut stats = ohl_pointcloud::Stats::default();
            if need_ground && !ground_classes.is_empty() {
                stats = source.visit(
                    Some(&corridor),
                    Some(&ground_classes),
                    &budget,
                    &mut |x, y, z, _| ground_points.add(x, y, z),
                )?;
            }
            let whole: Vec<u8> = class_counts
                .iter()
                .filter(|(_, n)| *n <= SMALL_CLASS)
                .map(|(c, _)| *c)
                .collect();
            let mut seen = [0u64; 256];
            let sample =
                source.visit_sample(1.0 / stride as f64, &whole, &budget, &mut |x, y, z, c| {
                    seen[c as usize] += 1;
                    points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c));
                })?;
            if known.is_none() {
                // Counts estimated from the sample.
                class_counts = (0..=255u8)
                    .filter(|c| seen[*c as usize] > 0)
                    .map(|c| (c, seen[c as usize] * stride as u64))
                    .collect();
            }
            stats.points_read += sample.points_read;
            stats.points_visited += sample.points_visited;
            stats
        };
        step(
            format!(
                "points: {} decoded of {} in {:.1?}; {} shown, {} ground cells",
                stats.points_read,
                total,
                t.elapsed(),
                points.len(),
                ground_points.points().len()
            ),
            &mut log,
        );
        if ground.is_none() && ground_points.points().len() >= 3 {
            let t = Instant::now();
            let g = ground_points
                .terrain(Some(build.max_edge))
                .map_err(|e| e.to_string())?;
            step(
                format!(
                    "ground: {} triangles from LiDAR in {:.1?}",
                    g.num_triangles(),
                    t.elapsed()
                ),
                &mut log,
            );
            ground = Some(g);
            ground_from_lidar = true;
        }
    }
    // The surface of the checks (the project's choice).
    let check_surface = session.as_ref().and_then(crate::surfaces::check_surface);
    let check_ground = match &check_surface {
        None => None,
        Some(n) if Some(n) == surface_settings.active.as_ref() && ground.is_some() => None,
        Some(n) => match crate::surfaces::load(&sources.file, n) {
            Ok(g) => Some(std::sync::Arc::new(g)),
            Err(e) => {
                step(format!("surface of the checks {n} not read: {e}"), &mut log);
                None
            }
        },
    };
    let (editing, viewer_session) = editing(session, sources.user.as_deref());
    let points_percent = sources
        .view
        .as_ref()
        .and_then(|v| v.lidar_percent)
        .or_else(|| {
            editing
                .as_ref()
                .ok()
                .and_then(|e| e.session.setting(LIDAR_PERCENT_SETTING).ok().flatten())
                .and_then(|v| v.parse::<f64>().ok())
        })
        .filter(|v| *v > 0.0 && *v <= 100.0)
        .unwrap_or(DEFAULT_LIDAR_PERCENT);
    // Remember the weather case file with the project.
    if let (Ok(e), Some(p)) = (&editing, &sources.weather) {
        remember_weather(&e.session, p);
    }
    match &editing {
        Ok(e) => step(format!("editing as {}", e.user), &mut log),
        Err(why) => step(format!("read-only: {why}"), &mut log),
    }
    step(format!("ready in {:.1?}", started.elapsed()), &mut log);
    let hidden_classes = sources
        .view
        .as_ref()
        .map(|v| v.hidden_classes.clone())
        .unwrap_or_default();
    let cases_shown = sources
        .view
        .as_ref()
        .map_or_else(CasesShown::default, |v| v.cases_shown);
    let section_display = sources
        .view
        .as_ref()
        .map(|v| v.section_display.clone())
        .unwrap_or_default();
    let backgrounds = crate::backgrounds::load(&sources.file, &mut log);
    let hidden_backgrounds = sources
        .view
        .as_ref()
        .map(|v| v.hidden_backgrounds.clone())
        .unwrap_or_default();
    let profile_ratio = sources
        .view
        .as_ref()
        .and_then(|v| v.profile_ratio)
        .filter(|k| k.is_finite() && *k > 0.0);
    let vegetation_colors = sources
        .view
        .as_ref()
        .and_then(|v| v.vegetation_colors)
        .unwrap_or([crate::scene::GROW_IN, crate::scene::FALL_IN]);
    let work_route = sources
        .view
        .as_ref()
        .and_then(|v| v.work_route.clone())
        .filter(|r| line.routes.iter().any(|x| &x.id == r));
    let shown_lines = sources
        .view
        .as_ref()
        .map(|v| v.shown_lines.clone())
        .unwrap_or_default();
    // The types' bodies as drawn, their profiles from the section library
    // the structure models read ([`Model::sections_folder`]).
    let bodies = drawn_bodies(
        types.as_ref(),
        &library_dir
            .as_ref()
            .map(|w| w.join(library_folder("sections")))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| sections_dir(&sources)),
    );
    let mut model = Model {
        plugin_columns: Vec::new(),
        check_plugins: Vec::new(),
        checks: Vec::new(),
        check_problems: Vec::new(),
        sources,
        line,
        cases: solved,
        ground: ground.map(std::sync::Arc::new),
        check_surface,
        check_ground,
        ground_source,
        surface_settings,
        other_surfaces,
        cloud,
        backgrounds,
        hidden_backgrounds,
        profile_ratio,
        vegetation_colors,
        drawings_setting_changed: false,
        work_route,
        points,
        origin,
        scheme,
        log,
        library,
        bodies,
        types,
        editing,
        display_cases: Default::default(),
        shown_cases: vec![0],
        selection_case: None,
        usage_marks: Default::default(),
        usage_out_of_date: false,
        other_lines: Vec::new(),
        lines_changed: Vec::new(),
        hidden_classes,
        class_counts,
        points_changed: None,
        ground_from_lidar,
        revisions: Default::default(),
        points_percent,
        library_dir,
        section_display,
        wind,
        cases_shown,
        viewer_session,
        remote: None,
        solving: Vec::new(),
        line_version: 0,
    };
    // The project lines this user showed (those that still exist).
    for name in shown_lines {
        if model
            .session()
            .is_some_and(|s| s.is_line(&name) && s.current_line() != name)
            && let Err(e) = model.show_line(&name)
        {
            log::warn!("project line {name}: {e}");
        }
    }
    Ok(model)
}
