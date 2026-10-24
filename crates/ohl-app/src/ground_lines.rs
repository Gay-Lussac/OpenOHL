// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! View › **Lines on the ground beside the line**: lines on the ground a
//! given distance away from the line, round **every span** of it — every
//! section of every route and branch, and the spans to structures beside
//! the alignments (a substation gantry, a portal, a tap) — measured from
//! its outermost conductor (the default) or from its axis (the
//! alignments). The conductors are those of the weather cases
//! **displayed** (owner, 2026-10-22: View › Display weather cases, each
//! section in its own display cases where the Sections table gives them,
//! [`Displayed`]): the plan envelope over every case displayed, each with
//! its wind's blowout from **both** sides when it has wind (the side the
//! views do not show solved here, in the background:
//! [`GroundLines::frame`]), the jumpers' solved positions counting as
//! conductors, so the lines go on round the structures — and, with the
//! "Other project lines" layer shown, those of the **project lines
//! displayed** next to the working line, each as it is shown (its sections
//! in the same display cases, both wind sides; [`ProjectLine`]).
//!
//! In plan the lines are the outline of the band within the distance of
//! the conductors (or of the axes), the line taken as a graph of its
//! spans ([`Graph`], [`outline`]): along each span, across its own chord,
//! the point farthest out on each side that is still within the distance
//! of a wire of that span; round the outer corner of an angle structure
//! in steps; closed round an end (the outline of its wires' ends widened
//! by the distance); joined at the structures, whatever lies within the
//! distance of another span cut off ([`clip`]) and the pieces joined
//! where they cut each other off ([`stitch`]), the loops where a line
//! still crosses itself cut off ([`without_loops_tagged`]): the outer
//! envelope, never crossing itself — over every route drawn and every
//! project line displayed, or each route and project line its own
//! ([`RouteLines`], [`Graph::add`]). It is draped on the ground
//! ([`drape_tagged`]): the active
//! terrain surface (TIN), else the ground built from the LiDAR ground
//! classes of the whole corridor (not the points drawn: the ground the
//! checks use, [`crate::model::Model::ground`]), at every edge of its
//! triangles crossed, broken where no ground lies under it (the gaps
//! counted, and a note when the lines go beyond the LiDAR corridor:
//! [`beyond_note`]); a LiDAR corridor wider than the model's chosen
//! ([`Settings::lidar_corridor`], at most [`MAX_CORRIDOR`]): a LiDAR
//! ground of the lines' own, built in the background and kept by width
//! ([`GroundLines::frame`]) — the model's ground, the checks', is never
//! changed; without either, the heights of the alignment's points,
//! every 2 m — and the menu offers to build a terrain surface from the
//! LiDAR when there is a point cloud ([`Drawn::offer_build`]).
//!
//! Display only: kept per set of weather cases (and project lines) and made again when the
//! line, the solutions, the ground or the settings change; never written
//! to a result or the project. The settings are this user's
//! ([`Settings`], remembered per project on this computer with the other
//! view choices).

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use eframe::egui;
use ohl_terrain::Terrain;
use ohl_weather::{WeatherCase, WindSide};

use crate::model::{Model, Solved};

/// What the distance is measured from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum From {
    /// The outermost conductor of the weather cases displayed, in plan
    /// (with its blowout from both sides): every point of every wire and
    /// jumper in those cases.
    #[default]
    OutermostConductor,
    /// The line's axis: the alignment.
    Axis,
}

/// How the lines of several routes are drawn.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RouteLines {
    /// One envelope over every route ticked and every project line
    /// displayed: where two routes (or lines) run side by side or cross,
    /// each side line is the outer edge of both.
    #[default]
    Envelope,
    /// Each route, and each project line displayed, its own lines
    /// (crossing those of another where they run side by side).
    Each,
}

/// This user's choices (View › Lines on the ground beside the line),
/// remembered per project on this computer ([`crate::model::ViewPrefs`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Drawn.
    pub on: bool,
    /// The distance from the line, m.
    pub distance: f64,
    /// What it is measured from.
    pub from: From,
    /// Routes left out (none: every route; a new route is drawn).
    pub hidden_routes: BTreeSet<String>,
    /// One envelope over the routes drawn, or each route its own lines.
    pub route_lines: RouteLines,
    /// Colour.
    pub color: [u8; 3],
    /// Width, pixels.
    pub width: f32,
    /// Dashed.
    pub dashed: bool,
    /// Also in the profile: the ground along each line (right near,
    /// left far).
    pub profile: bool,
    /// The half width of the LiDAR ground the lines are draped on, m from
    /// the spans' chords (as the model's corridor, `--corridor`); none:
    /// the model's corridor, its ground. Wider: a ground of its own for
    /// the lines, built in the background ([`GroundLines::frame`]); the
    /// model's ground, the checks', is not changed. At most
    /// [`MAX_CORRIDOR`].
    pub lidar_corridor: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            on: false,
            distance: DEFAULT_DISTANCE,
            from: From::OutermostConductor,
            hidden_routes: BTreeSet::new(),
            route_lines: RouteLines::Envelope,
            color: COLORS[0],
            width: 2.0,
            dashed: false,
            profile: false,
            lidar_corridor: None,
        }
    }
}

impl Settings {
    /// As by default (not remembered then).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// A hash of what the geometry depends on (not the style; the cases
    /// displayed are hashed by index in [`key`]).
    fn geometry_key(&self, h: &mut impl Hasher) {
        self.distance.to_bits().hash(h);
        self.from.hash(h);
        self.hidden_routes.hash(h);
        self.route_lines.hash(h);
        self.lidar_corridor.map(f64::to_bits).hash(h);
    }

    /// The LiDAR corridor of the lines, m: the one chosen (at most
    /// [`MAX_CORRIDOR`]), else `model`'s.
    pub fn corridor(&self, model: &Model) -> f64 {
        self.lidar_corridor
            .filter(|w| w.is_finite())
            .map_or(model.sources.corridor, |w| w.clamp(0.0, MAX_CORRIDOR))
    }

    /// The half width of a LiDAR ground of the lines' own, m: when the
    /// lines lie on the LiDAR ground of `model` (no terrain surface) and
    /// the corridor chosen is wider than the model's; none: the model's
    /// ground.
    pub fn wider(&self, model: &Model) -> Option<f64> {
        let w = self.corridor(model);
        (matches!(Ground::of(model), Ground::Lidar(_))
            && model.cloud.is_some()
            && w > model.sources.corridor)
            .then_some(w)
    }
}

/// The weather cases the lines are made from: those **displayed** (owner,
/// 2026-10-22; View › Display weather cases, [`Model::shown`]; a section
/// drawn in its own display cases, [`Model::section_cases`], in those),
/// indices into the model's cases. Old settings that chose cases of
/// their own (`shown_case`, `cases`) are read as displayed: those fields
/// are no longer read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Displayed {
    /// The cases shown for the whole line: the jumpers', and every
    /// section's without cases of its own.
    pub line: Vec<usize>,
    /// The sections drawn in cases of their own.
    pub sections: std::collections::BTreeMap<String, Vec<usize>>,
}

impl Displayed {
    /// What `model` displays, `shown` the case shown.
    pub fn of(model: &Model, shown: usize) -> Self {
        let line = model.shown(shown);
        let mut ids: BTreeSet<&str> = model.line.sections.iter().map(|s| s.id.as_str()).collect();
        ids.extend(model.display_cases.keys().map(String::as_str));
        let sections = ids
            .into_iter()
            .filter_map(|id| {
                let own = model.section_cases(id, shown);
                (own != line).then(|| (id.to_owned(), own))
            })
            .collect();
        Self { line, sections }
    }

    /// The cases `cases` for every section and jumper.
    #[cfg(test)]
    pub fn all(cases: &[usize]) -> Self {
        Self {
            line: cases.to_vec(),
            sections: Default::default(),
        }
    }

    /// Every case displayed anywhere, in order, each once.
    pub fn cases(&self) -> Vec<usize> {
        let all: BTreeSet<usize> = self
            .line
            .iter()
            .chain(self.sections.values().flatten())
            .copied()
            .collect();
        all.into_iter().collect()
    }

    /// Whether the spans of section `section` (of any line displayed, by
    /// id as the views take it) count in case `k`; `None`: the jumpers.
    pub fn counts(&self, section: Option<&str>, k: usize) -> bool {
        section
            .and_then(|id| self.sections.get(id))
            .unwrap_or(&self.line)
            .contains(&k)
    }
}

/// A project line displayed next to the working line (the "Other project
/// lines" layer shown, [`crate::model::OtherLine`]), as the lines take it.
#[derive(Debug, Clone, Copy)]
pub struct ProjectLine<'a> {
    /// Its name.
    pub name: &'a str,
    /// The line.
    pub line: &'a ohl_line::Line,
    /// Solved in each of the model's weather cases (the same order).
    pub cases: &'a [Solved],
    /// When it was shown ([`crate::model::OtherLine::serial`]).
    pub serial: u64,
}

impl<'a> ProjectLine<'a> {
    /// The project lines `model` displays.
    pub fn of(model: &'a Model) -> Vec<Self> {
        model
            .other_lines
            .iter()
            .map(|o| Self {
                name: &o.name,
                line: &o.line,
                cases: &o.cases,
                serial: o.serial,
            })
            .collect()
    }
}

/// Whether `case` has wind (then both its sides count).
fn windy(case: &WeatherCase) -> bool {
    case.wind_pressure.si() > 0.0
}

/// `case` with the wind from the other side.
pub fn other_side(case: &WeatherCase) -> WeatherCase {
    let mut other = case.clone();
    other.wind_from = match case.wind_from {
        WindSide::Left => WindSide::Right,
        WindSide::Right => WindSide::Left,
    };
    other
}

/// The windy weather cases chosen whose other wind side the model does
/// not hold (the views show one side): (index, that side's case).
pub fn other_sides_wanted(model: &Model, chosen: &[usize]) -> Vec<(usize, WeatherCase)> {
    sides_wanted(&model.cases, chosen)
}

/// [`other_sides_wanted`] of a line solved in `cases`.
fn sides_wanted(cases: &[Solved], chosen: &[usize]) -> Vec<(usize, WeatherCase)> {
    chosen
        .iter()
        .filter_map(|&k| {
            let c = cases.get(k)?;
            (c.ready && windy(&c.case) && c.mirror.is_none()).then(|| (k, other_side(&c.case)))
        })
        .collect()
}

/// The other wind sides wanted, solved now (tests; the application
/// solves them in the background, [`GroundLines::frame`]).
#[cfg(test)]
pub fn other_sides_now(model: &Model, chosen: &[usize]) -> Vec<(usize, Arc<Solved>)> {
    other_sides_wanted(model, chosen)
        .into_iter()
        .map(|(k, case)| (k, Arc::new(Solved::new(&model.line, &model.library, case))))
        .collect()
}

/// The distance offered first, m (a display choice, no rule behind it).
pub const DEFAULT_DISTANCE: f64 = 10.0;

/// The largest distance accepted, m.
pub const MAX_DISTANCE: f64 = 2000.0;

/// The widest LiDAR corridor of the lines, m from the spans' chords.
/// Measured on synthetic clouds of 5.2 and 11.8 million points along
/// 1.9 km of spans (`wide_corridor_timings`, release build): 60 m, 0.24
/// million 1 m cells, 0.6–0.7 s; 120 m, 0.49 million, 1.6–1.7 s, about
/// 105–140 MB; 200 m, 0.77 million, 1.8–2.1 s, about 175–190 MB (some
/// 230 bytes per cell). The ground's memory grows with the width and the
/// line's length (at 200 m about 90 MB per km, 3.3 times the model's
/// ground at 60 m), while a line's corridor survey rarely holds ground
/// points farther out.
pub const MAX_CORRIDOR: f64 = 200.0;

/// The colours offered.
pub const COLORS: [[u8; 3]; 6] = [
    [255, 110, 200],
    [255, 200, 60],
    [120, 230, 255],
    [140, 255, 120],
    [255, 255, 255],
    [255, 90, 70],
];

/// The step along the axis between points of the lines, m.
pub const STEP: f64 = 2.0;

/// The most the cross direction turns between two points around an angle
/// of the axis, degrees.
const FAN_STEP_DEG: f64 = 5.0;

/// The side of the line, looking ahead along the alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Left.
    Left,
    /// Right (the near side in the profile).
    Right,
}

/// The lines on the ground round a route (or round every route drawn):
/// its pieces (closed round a part of the line; broken where they leave
/// the ground or no wire is within the distance), project coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundLine {
    /// The route (empty: the envelope over every route drawn).
    pub route: String,
    /// The pieces, on the ground.
    pub runs: Vec<Vec<[f64; 3]>>,
    /// For each point of each piece, the half span it lies beside
    /// ([`Piece::tags`]; what that is: [`Drawn::beside`]).
    pub tags: Vec<Vec<u32>>,
}

/// Where the heights come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ground {
    /// The ground surface: a terrain surface (TIN) or survey file, by its
    /// name.
    Surface(String),
    /// The ground built from the LiDAR ground classes of the corridor:
    /// its points (the lowest of each cell).
    Lidar(usize),
    /// No ground surface: the heights of the alignment's points.
    Alignment,
}

impl Ground {
    /// Where the heights come from, as the menu says it: "TIN (name)",
    /// "LiDAR ground classes (n points)" or "alignment heights only".
    pub fn label(&self) -> String {
        match self {
            Self::Surface(name) => format!("TIN ({name})"),
            Self::Lidar(n) => format!("LiDAR ground classes ({n} points)"),
            Self::Alignment => "alignment heights only".into(),
        }
    }

    /// Where the model's heights come from ([`Model::ground`]): the
    /// surface loaded as the ground, else the ground built from the
    /// LiDAR, else none.
    pub fn of(model: &Model) -> Self {
        match model.ground.as_deref() {
            None => Self::Alignment,
            Some(g) if model.ground_from_lidar && model.ground_source.is_empty() => {
                Self::Lidar(g.points().len())
            }
            Some(_) => Self::Surface(surface_name(&model.ground_source)),
        }
    }
}

/// The name of the surface `source` says the ground is from
/// ([`Model::ground_source`]: "surface NAME" or "survey file PATH").
fn surface_name(source: &str) -> String {
    if let Some(name) = source.strip_prefix("surface ") {
        return name.to_owned();
    }
    if let Some(path) = source.strip_prefix("survey file ") {
        return std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
    }
    if source.is_empty() {
        "the ground shown".into()
    } else {
        source.to_owned()
    }
}

/// Why the model has no ground from its LiDAR points, in words (none
/// without a point cloud).
fn no_lidar_ground(model: &Model) -> Option<String> {
    model.cloud.as_ref()?;
    let classes = model.scheme.codes_with_role("ground");
    Some(if classes.is_empty() {
        format!(
            "the LiDAR class scheme ({}) has no class with the ground role",
            model.scheme.name
        )
    } else if !model
        .class_counts
        .iter()
        .any(|(c, n)| *n > 0 && classes.contains(c))
    {
        format!(
            "the point cloud has no point of a ground class ({})",
            classes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        "no point of the LiDAR ground classes within the corridor read".into()
    })
}

/// The lines of one set of weather cases.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    /// The lines (one per route, or one round every route drawn).
    pub lines: Vec<GroundLine>,
    /// By tag ([`GroundLine::tags`]): the alignment and its side a point
    /// lies beside (none: round an end, or beside a span away from the
    /// alignments).
    pub beside: Vec<Option<(String, Side)>>,
    /// The alignments' axes, by id (for the profile's stations and the
    /// heights without a ground).
    pub axes: Vec<(String, Vec<[f64; 3]>)>,
    /// Where the heights come from.
    pub ground: Ground,
    /// The stretches of the lines with no ground under them (beyond the
    /// surface's edge, outside the LiDAR corridor, or where the ground
    /// points are farther apart than the ground's longest triangle side):
    /// the lines are broken there.
    pub gaps: usize,
    /// The half width of the LiDAR ground the lines lie on, m from the
    /// spans' chords (none: not on the LiDAR ground).
    pub corridor: Option<f64>,
    /// Neither a terrain surface nor the LiDAR ground lies under the
    /// lines, and the model has a point cloud: why, in words — the menu
    /// offers to build a terrain surface from the LiDAR.
    pub offer_build: Option<String>,
    /// What could not be drawn, in words.
    pub notes: Vec<String>,
}

/// The other wind side of a windy weather case displayed, solved here when
/// the views show one side only.
#[derive(Debug)]
struct Other {
    /// Of the working line (none) or of a project line displayed (its
    /// name).
    line: Option<String>,
    /// The weather case's index.
    case: usize,
    /// The case solved (its wind from the other side).
    weather: WeatherCase,
    /// The line's version it is solved for (a project line: when it was
    /// shown, [`crate::model::OtherLine::serial`]).
    version: u64,
    /// Solving, or solved.
    state: OtherState,
}

#[derive(Debug)]
enum OtherState {
    /// Being solved for this line (of the version it was started for);
    /// the solution before drawn meanwhile.
    Solving(
        std::sync::mpsc::Receiver<Solved>,
        Option<Arc<Solved>>,
        Arc<ohl_line::Line>,
    ),
    /// Solved, for this line.
    Done(Arc<Solved>, Arc<ohl_line::Line>),
    /// To be solved again from the start (its solve stopped); the
    /// solution before drawn meanwhile.
    Waiting(Option<Arc<Solved>>),
}

/// `case` solved for `line`: from the solution `start` (solved for its
/// line) re-solving only the sections that differ (their own data, or a
/// structure they hang on), as the line worked on is after an edit; else
/// every section.
fn bring_up_to_date(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    case: WeatherCase,
    start: Option<(Arc<Solved>, Arc<ohl_line::Line>)>,
) -> Solved {
    match start {
        Some((s, from)) => {
            let dirty: Vec<String> =
                crate::model::Differences::between_with(&from, line, &Default::default())
                    .sections
                    .into_iter()
                    .collect();
            let mut live = s.live.clone();
            live.update(line, library, &dirty);
            Solved::from_live(line, case, live)
        }
        None => Solved::new(line, library, case),
    }
}

/// A LiDAR ground of the lines' own, wider than the model's.
#[derive(Debug)]
struct Wide {
    /// Its half width, m.
    width: f64,
    /// What it is built from ([`wide_source`]).
    source: u64,
    /// Building, or built.
    state: WideState,
}

type WideAnswer = Result<Option<Terrain>, String>;

#[derive(Debug)]
enum WideState {
    Building(std::sync::mpsc::Receiver<WideAnswer>),
    /// Built (none: no ground point in it), or why not.
    Done(Result<Option<Arc<Terrain>>, String>),
}

/// The wider LiDAR ground of the lines, this frame.
enum WideNow {
    /// None wanted: the model's ground.
    Off,
    /// Being built.
    Building(f64),
    /// Built.
    Ready(Arc<Terrain>, f64),
    /// Not built: why.
    Failed(f64, String),
}

/// What a wider LiDAR ground of the lines is built from: the point cloud,
/// its ground classes and their points' revision, the model's ground (built
/// again after a change of its points), the chords of the spans.
fn wide_source(model: &Model) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    model.cloud.hash(&mut h);
    model.scheme.codes_with_role("ground").hash(&mut h);
    model.revisions.ground_points.hash(&mut h);
    model
        .ground
        .as_ref()
        .map(|g| Arc::as_ptr(g) as usize)
        .hash(&mut h);
    for c in chords(&model.line) {
        for p in c {
            p.map(f64::to_bits).hash(&mut h);
        }
    }
    h.finish()
}

/// Wider LiDAR grounds kept (by width; each a TIN of the corridor).
const WIDE_KEPT: usize = 2;

/// The lines drawn: kept per set of weather cases (a few), made again
/// when what they come from changes.
#[derive(Debug, Default)]
pub struct GroundLines {
    /// Made for: the key of each, newest last.
    cache: Vec<(u64, Arc<Drawn>)>,
    /// What the views draw now (none: nothing).
    pub drawn: Option<Arc<Drawn>>,
    /// The style the views drew it with.
    style: Option<([u8; 3], u32, bool, bool)>,
    /// How many times the lines were made (tests).
    pub made: u64,
    /// The other wind sides of the windy cases chosen (those the model
    /// does not hold).
    others: Vec<Other>,
    /// The wider LiDAR grounds of the lines, newest last (never the
    /// model's ground: the checks keep theirs).
    wide: Vec<Wide>,
    /// How many wider LiDAR grounds were built (tests).
    pub wide_built: u64,
}

/// Sets of weather cases kept.
const CACHED: usize = 8;

impl GroundLines {
    /// Every frame: the lines of the weather cases displayed (`shown` the
    /// case shown, [`Displayed::of`]) and, with `project_lines` (the
    /// "Other project lines" layer shown), of the project lines displayed,
    /// as `settings` say (made when not kept yet). The other wind side of
    /// a windy case displayed that the model (a project line) does not
    /// hold is solved in the background (`ctx` repainted when done; none:
    /// solved now); until then the lines take what is solved and say what
    /// is not. Whether the views must draw them again.
    pub fn frame(
        &mut self,
        model: &Model,
        shown: usize,
        settings: &Settings,
        project_lines: bool,
        ctx: Option<&egui::Context>,
    ) -> bool {
        if !settings.on {
            let changed = self.drawn.is_some();
            self.drawn = None;
            self.style = None;
            return changed;
        }
        let displayed = Displayed::of(model, shown);
        let chosen = displayed.cases();
        let projects = if project_lines {
            ProjectLine::of(model)
        } else {
            Vec::new()
        };
        if settings.from == From::OutermostConductor {
            self.solve_others(model, &chosen, &projects, ctx);
        }
        let done = |line: Option<&str>| -> Vec<(usize, Arc<Solved>)> {
            self.others
                .iter()
                .filter(|o| o.line.as_deref() == line)
                .filter_map(|o| match &o.state {
                    OtherState::Done(s, _)
                    | OtherState::Solving(_, Some(s), _)
                    | OtherState::Waiting(Some(s)) => Some((o.case, s.clone())),
                    OtherState::Solving(_, None, _) | OtherState::Waiting(None) => None,
                })
                .collect()
        };
        let others = done(None);
        let project_others: Vec<Vec<(usize, Arc<Solved>)>> =
            projects.iter().map(|p| done(Some(p.name))).collect();
        let wide = self.wide_ground(model, settings, ctx);
        let mut key = key(
            model,
            &displayed,
            &others,
            &projects,
            &project_others,
            settings,
        );
        {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut h);
            match &wide {
                WideNow::Off => 0u8.hash(&mut h),
                WideNow::Building(w) => (1u8, w.to_bits()).hash(&mut h),
                WideNow::Ready(g, w) => (2u8, Arc::as_ptr(g) as usize, w.to_bits()).hash(&mut h),
                WideNow::Failed(w, e) => (3u8, w.to_bits(), e).hash(&mut h),
            }
            key = h.finish();
        }
        let drawn = match self.cache.iter().position(|(k, _)| *k == key) {
            Some(i) => {
                let entry = self.cache.remove(i);
                let d = entry.1.clone();
                self.cache.push(entry);
                d
            }
            None => {
                let on = match &wide {
                    WideNow::Ready(g, w) => Some((&**g, *w)),
                    _ => None,
                };
                let mut d = compute_on(
                    model,
                    &displayed,
                    &others,
                    &projects,
                    &project_others,
                    settings,
                    on,
                );
                let mc = model.sources.corridor;
                match &wide {
                    WideNow::Building(w) => d.notes.push(format!(
                        "the LiDAR ground within {w:.0} m of the spans is being built (until \
                         then the lines lie on the model's, ±{mc:.0} m)"
                    )),
                    WideNow::Failed(w, e) => d.notes.push(format!(
                        "the LiDAR ground within {w:.0} m of the spans not built: {e} (the \
                         lines lie on the model's, ±{mc:.0} m)"
                    )),
                    _ => {}
                }
                for o in &self.others {
                    if !matches!(o.state, OtherState::Done(..)) {
                        d.notes.push(match &o.line {
                            None => format!(
                                "weather case {}: the wind from the other side is being solved",
                                o.weather.name
                            ),
                            Some(name) => format!(
                                "project line {name}, weather case {}: the wind from the other \
                                 side is being solved",
                                o.weather.name
                            ),
                        });
                    }
                }
                let d = Arc::new(d);
                self.made += 1;
                // A line changed: what was kept for the other sets goes
                // the next time they are shown (their key differs).
                self.cache.push((key, d.clone()));
                if self.cache.len() > CACHED {
                    self.cache.remove(0);
                }
                d
            }
        };
        let style = Some((
            settings.color,
            settings.width.to_bits(),
            settings.dashed,
            settings.profile,
        ));
        let changed =
            self.drawn.as_ref().is_none_or(|d| !Arc::ptr_eq(d, &drawn)) || self.style != style;
        self.drawn = Some(drawn);
        self.style = style;
        changed
    }

    /// The other wind sides wanted for `chosen`, of the working line and
    /// of the project lines `projects`: kept when solved for that line
    /// (its version); else brought up to date from the solution before
    /// (only the sections that differ re-solved; it stays drawn
    /// meanwhile), one solve at a time per case (in the background with
    /// `ctx`, else now); those finished taken; those no longer wanted
    /// dropped.
    fn solve_others(
        &mut self,
        model: &Model,
        chosen: &[usize],
        projects: &[ProjectLine<'_>],
        ctx: Option<&egui::Context>,
    ) {
        // (the line's name, the line, its version, the sides wanted).
        type Wanted<'a> = (
            Option<&'a str>,
            &'a ohl_line::Line,
            u64,
            Vec<(usize, WeatherCase)>,
        );
        let mut lines: Vec<Wanted<'_>> = vec![(
            None,
            &model.line,
            model.line_version,
            other_sides_wanted(model, chosen),
        )];
        for p in projects {
            lines.push((
                Some(p.name),
                p.line,
                p.serial,
                sides_wanted(p.cases, chosen),
            ));
        }
        // Kept while wanted: solved for an older version, drawn until it
        // is brought up to date.
        self.others.retain(|o| {
            lines.iter().any(|(name, _, _, wanted)| {
                o.line.as_deref() == *name
                    && wanted.iter().any(|(k, w)| *k == o.case && *w == o.weather)
            })
        });
        // Those finished taken (solved for the line they were started
        // for: brought up to date below when it changed since).
        for o in &mut self.others {
            let next = match &o.state {
                OtherState::Solving(rx, before, from) => match rx.try_recv() {
                    Ok(s) => Some(OtherState::Done(Arc::new(s), from.clone())),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        Some(OtherState::Waiting(before.clone()))
                    }
                },
                _ => None,
            };
            if let Some(n) = next {
                o.state = n;
            }
        }
        for (name, line, version, wanted) in &lines {
            for (k, weather) in wanted {
                let at = self
                    .others
                    .iter()
                    .position(|o| o.line.as_deref() == *name && o.case == *k);
                // Solved for this version, or being solved (one at a time:
                // the next started when it is in); else started, from
                // the solution before when there is one.
                let (before, from) = match at.map(|i| &self.others[i]) {
                    Some(o) => match &o.state {
                        OtherState::Solving(..) => continue,
                        OtherState::Done(..) if o.version == *version => continue,
                        OtherState::Done(s, l) => (Some(s.clone()), Some(l.clone())),
                        OtherState::Waiting(b) => (b.clone(), None),
                    },
                    None => (None, None),
                };
                let state = match ctx {
                    Some(ctx) => {
                        let (line, library) = ((*line).clone(), model.library.clone());
                        let (tx, rx) = std::sync::mpsc::channel();
                        let (ctx, case) = (ctx.clone(), weather.clone());
                        let start = before.clone().zip(from);
                        let started_for = Arc::new(line.clone());
                        std::thread::spawn(move || {
                            let _ = tx.send(bring_up_to_date(&line, &library, case, start));
                            ctx.request_repaint();
                        });
                        OtherState::Solving(rx, before, started_for)
                    }
                    None => OtherState::Done(
                        Arc::new(bring_up_to_date(
                            line,
                            &model.library,
                            weather.clone(),
                            before.zip(from),
                        )),
                        Arc::new((*line).clone()),
                    ),
                };
                let o = Other {
                    line: name.map(str::to_owned),
                    case: *k,
                    weather: weather.clone(),
                    version: *version,
                    state,
                };
                match at {
                    Some(i) => self.others[i] = o,
                    None => self.others.push(o),
                }
            }
        }
    }

    /// The wider LiDAR ground `settings` ask for in `model`
    /// ([`Settings::wider`]): kept when built from the same, else built —
    /// in the background with `ctx` (one at a time; `ctx` repainted when
    /// done), else now. Those built from another cloud, ground or line
    /// dropped; [`WIDE_KEPT`] kept.
    fn wide_ground(
        &mut self,
        model: &Model,
        settings: &Settings,
        ctx: Option<&egui::Context>,
    ) -> WideNow {
        for w in &mut self.wide {
            if let WideState::Building(rx) = &w.state {
                match rx.try_recv() {
                    Ok(r) => w.state = WideState::Done(r.map(|g| g.map(Arc::new))),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        w.state = WideState::Done(Err("the build stopped".into()));
                    }
                }
            }
        }
        let Some(width) = settings.wider(model) else {
            return WideNow::Off;
        };
        let source = wide_source(model);
        // Built from another cloud, ground or line: of no use (one being
        // built is left to finish, then dropped).
        self.wide
            .retain(|w| w.source == source || matches!(w.state, WideState::Building(_)));
        let building = self
            .wide
            .iter()
            .any(|w| matches!(w.state, WideState::Building(_)));
        if let Some(i) = self
            .wide
            .iter()
            .position(|w| w.source == source && w.width == width)
        {
            // Newest last.
            let w = self.wide.remove(i);
            let now = match &w.state {
                WideState::Building(_) => WideNow::Building(width),
                WideState::Done(Ok(Some(g))) => WideNow::Ready(g.clone(), width),
                WideState::Done(Ok(None)) => {
                    WideNow::Failed(width, "no point of the ground classes in it".into())
                }
                WideState::Done(Err(e)) => WideNow::Failed(width, e.clone()),
            };
            self.wide.push(w);
            return now;
        }
        if building {
            // The next once the one under way is done.
            return WideNow::Building(width);
        }
        let (Some(spec), classes) = (model.cloud.clone(), model.scheme.codes_with_role("ground"))
        else {
            return WideNow::Off;
        };
        let line = model.line.clone();
        self.wide_built += 1;
        let state = match ctx {
            Some(ctx) => {
                let (tx, rx) = std::sync::mpsc::channel();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::lidar_edit::lidar_ground(
                        &spec, &classes, &line, width,
                    ));
                    ctx.request_repaint();
                });
                WideState::Building(rx)
            }
            None => WideState::Done(
                crate::lidar_edit::lidar_ground(&spec, &classes, &line, width)
                    .map(|g| g.map(Arc::new)),
            ),
        };
        self.wide.push(Wide {
            width,
            source,
            state,
        });
        // The oldest built dropped beyond those kept.
        while self.wide.len() > WIDE_KEPT {
            let Some(i) = self
                .wide
                .iter()
                .position(|w| !matches!(w.state, WideState::Building(_)))
            else {
                break;
            };
            self.wide.remove(i);
        }
        // Built now: taken at once.
        self.wide_ground(model, settings, None)
    }

    /// Whether a wider LiDAR ground is being built (tests).
    #[cfg(test)]
    pub fn building(&self) -> bool {
        self.wide
            .iter()
            .any(|w| matches!(w.state, WideState::Building(_)))
    }

    /// Whether an other wind side is still being solved (tests).
    #[cfg(test)]
    pub fn solving(&self) -> bool {
        self.others
            .iter()
            .any(|o| !matches!(o.state, OtherState::Done(..)))
    }
}

/// What the lines of the cases `displayed` are made from: the settings'
/// geometry, the line (its version, the alignments), the solutions of
/// the cases (their clamps and low points, both wind sides, the other
/// sides `others` solved here), the project lines displayed (`projects`,
/// when each was shown, their solutions and other sides
/// `project_others`), the ground.
fn key(
    model: &Model,
    displayed: &Displayed,
    others: &[(usize, Arc<Solved>)],
    projects: &[ProjectLine<'_>],
    project_others: &[Vec<(usize, Arc<Solved>)>],
    settings: &Settings,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    settings.geometry_key(&mut h);
    displayed.hash(&mut h);
    model.line_version.hash(&mut h);
    for p in &model.line.alignment_points {
        (&p.id, p.position.map(f64::to_bits)).hash(&mut h);
    }
    for a in &model.line.alignments {
        (&a.id, &a.route, &a.points).hash(&mut h);
    }
    for p in projects {
        (p.name, p.serial).hash(&mut h);
    }
    if settings.from == From::OutermostConductor {
        let chosen = displayed.cases();
        for &k in &chosen {
            model.cases.get(k).map(|c| c.ready).hash(&mut h);
        }
        let mut all = solutions(&model.cases, &chosen, others, None).0;
        for (p, o) in projects.iter().zip(project_others) {
            all.extend(solutions(p.cases, &chosen, o, Some(p.name)).0);
        }
        for (k, s) in all {
            k.hash(&mut h);
            (s.ready, s.case.wind_from == WindSide::Left).hash(&mut h);
            for shape in &s.shapes {
                shape.section.hash(&mut h);
                for p in &shape.clamps {
                    p.map(f64::to_bits).hash(&mut h);
                }
                for span in &shape.spans {
                    span.low_point.map(f64::to_bits).hash(&mut h);
                    span.mid_point.map(f64::to_bits).hash(&mut h);
                }
            }
            for j in &s.jumpers {
                j.low_point.map(f64::to_bits).hash(&mut h);
            }
        }
    }
    model
        .ground
        .as_ref()
        .map(|g| (Arc::as_ptr(g) as usize, g.num_triangles()))
        .hash(&mut h);
    // What the menu says of the ground (and why there is none).
    (&model.ground_source, model.ground_from_lidar).hash(&mut h);
    model.cloud.hash(&mut h);
    model.scheme.codes_with_role("ground").hash(&mut h);
    model.class_counts.hash(&mut h);
    h.finish()
}

/// The solutions of a line solved in `cases` (the model's, or a project
/// line's: `name`) the lines of the cases `chosen` are measured from:
/// each case solved (both its wind sides when it holds them) and the
/// other wind sides in `others` (by case index), each with its case's
/// index; with the cases not solved yet, in words.
pub fn solutions<'a>(
    cases: &'a [Solved],
    chosen: &[usize],
    others: &'a [(usize, Arc<Solved>)],
    name: Option<&str>,
) -> (Vec<(usize, &'a Solved)>, Vec<String>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for &k in chosen {
        let Some(c) = cases.get(k) else {
            continue;
        };
        if !c.ready {
            notes.push(match name {
                None => format!(
                    "weather case {}: not solved yet (taken when it is)",
                    c.case.name
                ),
                Some(name) => format!(
                    "project line {name}, weather case {}: not solved yet (taken when it is)",
                    c.case.name
                ),
            });
            continue;
        }
        out.extend(c.drawn().map(|s| (k, s)));
        out.extend(
            others
                .iter()
                .filter(|(j, _)| *j == k)
                .map(|(_, s)| (k, &**s)),
        );
    }
    (out, notes)
}

/// [`compute_on`] of the cases `chosen` for every section, without
/// project lines, on the model's ground (tests).
#[cfg(test)]
pub fn compute(
    model: &Model,
    chosen: &[usize],
    others: &[(usize, Arc<Solved>)],
    settings: &Settings,
) -> Drawn {
    compute_on(
        model,
        &Displayed::all(chosen),
        others,
        &[],
        &[],
        settings,
        None,
    )
}

/// The lines of the weather cases `displayed` (indices into the model's
/// cases, per section; `others`: the other wind side of windy ones the
/// model does not hold, solved by [`GroundLines::frame`]) and of the
/// project lines displayed (`projects`, each in the same cases, with its
/// other wind sides `project_others`): for each alignment of the routes
/// drawn, its two sides, draped on the ground — from the outermost
/// conductor, the plan envelope over every solution. With
/// [`RouteLines::Envelope`] the project lines' spans join the envelope (a
/// span on the same two structure places as one of the line's, one span
/// of both); with [`RouteLines::Each`] each project line has its own
/// lines. The ground: `wide` when given (a LiDAR ground of the lines' own
/// and its half width, m), else the model's.
#[allow(clippy::too_many_arguments)]
pub fn compute_on(
    model: &Model,
    displayed: &Displayed,
    others: &[(usize, Arc<Solved>)],
    projects: &[ProjectLine<'_>],
    project_others: &[Vec<(usize, Arc<Solved>)>],
    settings: &Settings,
    wide: Option<(&Terrain, f64)>,
) -> Drawn {
    let line = &model.line;
    let (ground, corridor) = match wide {
        Some((g, w)) => (Some(g), Some(w)),
        None => {
            let ground = Ground::of(model);
            let w = matches!(ground, Ground::Lidar(_)).then_some(model.sources.corridor);
            (model.ground.as_deref(), w)
        }
    };
    let mut out = Drawn {
        lines: Vec::new(),
        beside: Vec::new(),
        axes: Vec::new(),
        ground: match wide {
            Some((g, _)) => Ground::Lidar(g.points().len()),
            None => Ground::of(model),
        },
        gaps: 0,
        corridor,
        offer_build: None,
        notes: Vec::new(),
    };
    // The LiDAR ground's corridor, and how far beyond it the lines go
    // (m from the spans' chords).
    let reach = corridor.map(|w| (ohl_lidar::Corridor::along_spans(line, w), chords(line)));
    let mut farthest: f64 = 0.0;
    // Some line made in plan (to drape).
    let mut planned = false;
    let d = settings.distance.clamp(0.0, MAX_DISTANCE);
    let chosen = displayed.cases();
    if settings.from == From::OutermostConductor && chosen.is_empty() {
        out.notes
            .push("no weather case displayed: nothing to measure from".into());
        return out;
    }
    let keep = |section: Option<&str>, k: usize| displayed.counts(section, k);
    // The graph of a line: its axes, or its wires in the cases displayed.
    let mut graph_of = |line: &ohl_line::Line,
                        cases: &[Solved],
                        others: &[(usize, Arc<Solved>)],
                        name: Option<&str>,
                        views: &[ohl_line::route::AlignmentView]|
     -> Graph {
        match settings.from {
            From::Axis => Graph::of_axes(views),
            From::OutermostConductor => {
                let (s, notes) = solutions(cases, &chosen, others, name);
                out.notes.extend(notes);
                Graph::of_wires_in(line, &s, views, &keep)
            }
        }
    };
    let views = line.alignment_views();
    let mut graph = graph_of(line, &model.cases, others, None, &views);
    let envelope = settings.route_lines == RouteLines::Envelope;
    for (p, o) in projects.iter().zip(project_others) {
        let views = p.line.alignment_views();
        let g = graph_of(p.line, p.cases, o, Some(p.name), &views);
        graph.add(g, envelope, p.name);
    }
    if settings.from == From::OutermostConductor && graph.edges.is_empty() {
        out.notes
            .push("no wire solved in the weather cases displayed".into());
    }
    out.axes = views
        .iter()
        .map(|v| (v.id.clone(), v.centreline(line)))
        .filter(|(_, a)| a.len() > 1)
        .collect();
    out.beside = graph
        .edges
        .iter()
        .flat_map(|e| {
            [false, true].map(|back| {
                e.along.as_ref().map(|(id, ahead)| {
                    let side = if *ahead != back {
                        Side::Left
                    } else {
                        Side::Right
                    };
                    (id.clone(), side)
                })
            })
        })
        .collect();
    let axes: Vec<&[[f64; 3]]> = out.axes.iter().map(|(_, a)| a.as_slice()).collect();
    for (route, edges) in groups(&graph, settings) {
        let mut runs = Vec::new();
        let mut tags = Vec::new();
        for piece in outline(&graph, &edges, d) {
            if let Some((c, chords)) = &reach {
                for p in piece.points.iter().filter(|p| !c.contains(p[0], p[1])) {
                    farthest = farthest.max(beyond(chords, *p));
                }
            }
            let (draped, t, gaps) = drape_tagged(&piece.points, &piece.tags, ground, &axes);
            runs.extend(draped);
            tags.extend(t);
            out.gaps += gaps;
            planned = true;
        }
        out.lines.push(GroundLine { route, runs, tags });
    }
    let on_ground = out.lines.iter().any(|l| !l.runs.is_empty());
    match out.ground {
        Ground::Alignment => {
            out.offer_build = no_lidar_ground(model);
            if out.offer_build.is_none() {
                out.notes.push(
                    "no terrain surface and no LiDAR point cloud: import a surface in \
                     Terrain › Terrain surfaces (TIN)"
                        .into(),
                );
            }
        }
        _ if planned && !on_ground => {
            let ground = out.ground.label();
            out.offer_build = model
                .cloud
                .as_ref()
                .map(|_| format!("the ground ({ground}) does not reach the lines"));
            out.notes.push(format!(
                "no ground under the lines ({ground}): nothing drawn"
            ));
        }
        _ if out.gaps > 0 => out.notes.push(format!(
            "{} gap{} where no ground lies under the lines (broken there)",
            out.gaps,
            if out.gaps == 1 { "" } else { "s" }
        )),
        _ => {}
    }
    if let Some(w) = corridor
        && farthest > 0.0
    {
        out.notes.push(beyond_note(w, farthest));
    }
    out
}

/// The spans of `graph` drawn, as `settings` group them: (the route,
/// its spans) for each route drawn ([`RouteLines::Each`]: a span on
/// several routes with the first of them drawn), or ("", every span
/// drawn) ([`RouteLines::Envelope`]). A span is drawn when it is on no
/// route or on a route not left out.
fn groups(graph: &Graph, settings: &Settings) -> Vec<(String, Vec<usize>)> {
    let hidden = &settings.hidden_routes;
    let shown = |e: &Edge| e.routes.iter().find(|r| !hidden.contains(*r)).cloned();
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (k, e) in graph.edges.iter().enumerate() {
        let route = match shown(e) {
            Some(r) => r,
            None if e.routes.is_empty() => String::new(),
            None => continue,
        };
        let key = match settings.route_lines {
            RouteLines::Envelope => String::new(),
            RouteLines::Each => route,
        };
        match out.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(k),
            None => out.push((key, vec![k])),
        }
    }
    out
}

/// The chords of the line's spans in plan, as the LiDAR corridor is
/// built round them ([`ohl_lidar::Corridor::along_spans`]).
fn chords(line: &ohl_line::Line) -> Vec<Seg> {
    line.sections
        .iter()
        .flat_map(|s| {
            let points: Vec<[f64; 2]> = s
                .points
                .iter()
                .filter_map(|p| line.wire_point(p))
                .map(|w| [w.position[0], w.position[1]])
                .collect();
            points.windows(2).map(|w| [w[0], w[1]]).collect::<Vec<_>>()
        })
        .collect()
}

/// How far `p` lies from the nearest chord, m.
fn beyond(chords: &[Seg], p: [f64; 2]) -> f64 {
    chords
        .iter()
        .map(|s| ohl_core::geometry::distance_to_segment(p, s[0], s[1]))
        .fold(f64::INFINITY, f64::min)
}

/// The corridor that covers lines reaching `farthest` m from the spans:
/// 2 m more (the ground's last 1 m cells), up to the next 5 m.
pub fn corridor_for(farthest: f64) -> f64 {
    ((farthest + 2.0) / 5.0).ceil() * 5.0
}

/// What the menu says when the lines reach `farthest` m from the spans,
/// beyond the LiDAR corridor of half width `w`.
pub fn beyond_note(w: f64, farthest: f64) -> String {
    let needed = corridor_for(farthest);
    if w >= MAX_CORRIDOR {
        format!(
            "lines beyond the LiDAR corridor (±{w:.0} m, the widest) are broken: they reach \
             {farthest:.0} m from the spans"
        )
    } else if needed <= MAX_CORRIDOR {
        format!(
            "lines beyond the LiDAR corridor (±{w:.0} m) are broken; widen the corridor to \
             {needed:.0} m (up to {MAX_CORRIDOR:.0} m)"
        )
    } else {
        format!(
            "lines beyond the LiDAR corridor (±{w:.0} m) are broken; widen the corridor (up to \
             {MAX_CORRIDOR:.0} m): they reach {farthest:.0} m from the spans, beyond the widest"
        )
    }
}

/// A segment in plan.
pub type Seg = [[f64; 2]; 2];

/// What the lines are measured from: the line as a graph in plan — its
/// nodes (the structures, or the alignments' points), its edges (every
/// span between two of them, each with what lies in it) and at each node
/// what is held there (the jumpers).
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// The nodes, in plan.
    pub nodes: Vec<[f64; 2]>,
    /// The edges (spans).
    pub edges: Vec<Edge>,
    /// At node k: what it holds (jumpers), segments in plan.
    pub joints: Vec<Vec<Seg>>,
}

/// One span of the [`Graph`].
#[derive(Debug, Clone, Default)]
pub struct Edge {
    /// Its first node.
    pub a: usize,
    /// Its second node.
    pub b: usize,
    /// What lies in it (every wire of every case, each subconductor; from
    /// the axis: its chord), segments in plan.
    pub sources: Vec<Seg>,
    /// The routes it is on (both its ends' routes; none in common: either's).
    pub routes: Vec<String>,
    /// The alignment it runs along (its ends follow each other on it), and
    /// whether `a` to `b` is ahead (increasing stations); none: a span
    /// away from the alignments (to a structure beside the line).
    pub along: Option<(String, bool)>,
}

/// The segments of a polyline in plan (a single point: a segment of no
/// length).
fn segments(points: &[[f64; 3]], out: &mut Vec<Seg>) {
    if points.len() == 1 {
        out.push([[points[0][0], points[0][1]]; 2]);
    }
    for w in points.windows(2) {
        out.push([[w[0][0], w[0][1]], [w[1][0], w[1][1]]]);
    }
}

impl Graph {
    /// A path through `points` (k to k + 1 holding `spans[k]`, `joints[k]`
    /// held at point k), along an alignment "a", ahead (tests).
    #[cfg(test)]
    pub fn path(points: Vec<[f64; 2]>, spans: Vec<Vec<Seg>>, joints: Vec<Vec<Seg>>) -> Self {
        let edges = spans
            .into_iter()
            .enumerate()
            .map(|(k, sources)| Edge {
                a: k,
                b: k + 1,
                sources,
                routes: Vec::new(),
                along: Some(("a".into(), true)),
            })
            .collect();
        let mut joints = joints;
        joints.resize(points.len(), Vec::new());
        Self {
            nodes: points,
            edges,
            joints,
        }
    }

    /// The node of key `key` at `at` (made when new).
    fn node(&mut self, ids: &mut HashMap<String, usize>, key: &str, at: [f64; 2]) -> usize {
        *ids.entry(key.to_owned()).or_insert_with(|| {
            self.nodes.push(at);
            self.joints.push(Vec::new());
            self.nodes.len() - 1
        })
    }

    /// The edge between nodes `a` and `b` (made when new, from `a` to `b`).
    fn edge(&mut self, ids: &mut HashMap<(usize, usize), usize>, a: usize, b: usize) -> usize {
        *ids.entry((a.min(b), a.max(b))).or_insert_with(|| {
            self.edges.push(Edge {
                a,
                b,
                ..Edge::default()
            });
            self.edges.len() - 1
        })
    }

    /// The alignments `views` themselves: their points the nodes (a point
    /// shared by two alignments, one node), each leg an edge holding its
    /// chord.
    pub fn of_axes(views: &[ohl_line::route::AlignmentView]) -> Self {
        let mut g = Self::default();
        let (mut nodes, mut edges) = (HashMap::new(), HashMap::new());
        for v in views {
            let ks: Vec<usize> = v
                .alignment
                .iter()
                .map(|p| g.node(&mut nodes, &p.id, [p.position[0], p.position[1]]))
                .collect();
            for w in ks.windows(2) {
                if w[0] == w[1] {
                    continue;
                }
                let e = g.edge(&mut edges, w[0], w[1]);
                let (a, b) = (g.nodes[w[0]], g.nodes[w[1]]);
                let edge = &mut g.edges[e];
                if edge.sources.is_empty() {
                    edge.sources.push([a, b]);
                }
                if !edge.routes.contains(&v.route) {
                    edge.routes.push(v.route.clone());
                }
                if edge.along.is_none() {
                    edge.along = Some((v.id.clone(), edge.a == w[0]));
                }
            }
        }
        g
    }

    /// [`Graph::of_wires_in`] of every span and jumper of `solutions`
    /// (tests).
    #[cfg(test)]
    pub fn of_wires(
        line: &ohl_line::Line,
        solutions: &[&Solved],
        views: &[ohl_line::route::AlignmentView],
    ) -> Self {
        let all: Vec<(usize, &Solved)> = solutions.iter().map(|s| (0, *s)).collect();
        Self::of_wires_in(line, &all, views, &|_, _| true)
    }

    /// The wires of `solutions`, each with its case's index: every span
    /// of every section (between two different structures, one edge
    /// however many sections and cases it holds: each wire, each
    /// subconductor of a bundle), the jumpers at the structures they end
    /// at (a jumper on a tap in a span: with that span's wires) — a
    /// section's spans only where `keep(Some(section), case)`, the
    /// jumpers only where `keep(None, case)` (the cases displayed,
    /// [`Displayed::counts`]). The edges' routes and alignments from
    /// `line`'s alignments `views`.
    pub fn of_wires_in(
        line: &ohl_line::Line,
        solutions: &[(usize, &Solved)],
        views: &[ohl_line::route::AlignmentView],
        keep: &dyn Fn(Option<&str>, usize) -> bool,
    ) -> Self {
        let mut g = Self::default();
        let (mut nodes, mut edges) = (HashMap::new(), HashMap::new());
        let at = |id: &str| line.structure(id).map(|s| [s.position[0], s.position[1]]);
        for &(k, s) in solutions {
            for shape in &s.shapes {
                let Some(section) = line.sections.iter().find(|x| x.id == shape.section) else {
                    continue;
                };
                if !keep(Some(&shape.section), k) {
                    continue;
                }
                for (k, span) in shape.spans.iter().enumerate() {
                    let (Some(a), Some(b)) = (section.points.get(k), section.points.get(k + 1))
                    else {
                        continue;
                    };
                    let (Some(pa), Some(pb)) = (at(&a.structure), at(&b.structure)) else {
                        continue;
                    };
                    let i = g.node(&mut nodes, &a.structure, pa);
                    let j = g.node(&mut nodes, &b.structure, pb);
                    let out = if i == j {
                        &mut g.joints[i]
                    } else {
                        let e = g.edge(&mut edges, i, j);
                        &mut g.edges[e].sources
                    };
                    match s.bundles.spans.get(&(shape.section.clone(), k)) {
                        Some(subs) if !subs.is_empty() => {
                            for sub in subs {
                                segments(sub, out);
                            }
                        }
                        _ => segments(&span.points, out),
                    }
                }
            }
        }
        for &(k, s) in solutions {
            if !keep(None, k) {
                continue;
            }
            for j in &s.jumpers {
                let Some(def) = line.jumpers.iter().find(|x| x.id == j.jumper) else {
                    continue;
                };
                let mut segs = Vec::new();
                match s.bundles.jumpers.get(&j.jumper) {
                    Some(subs) if !subs.is_empty() => {
                        for sub in subs {
                            segments(sub, &mut segs);
                        }
                    }
                    _ => segments(&j.points, &mut segs),
                }
                for end in [&def.from, &def.to] {
                    let ids = line.end_structures(end);
                    let ks: Vec<usize> =
                        ids.iter().filter_map(|id| nodes.get(id).copied()).collect();
                    match (end, ks.as_slice()) {
                        // A tap in a span: with its wires.
                        (ohl_line::JumperEnd::Tap { .. }, [a, b]) if a != b => {
                            if let Some(&e) = edges.get(&((*a).min(*b), (*a).max(*b))) {
                                g.edges[e].sources.extend_from_slice(&segs);
                            }
                        }
                        (_, [k]) => g.joints[*k].extend_from_slice(&segs),
                        _ => {}
                    }
                }
            }
        }
        // Routes and alignments.
        let routes = line.structure_routes();
        let ids: Vec<&String> = {
            let mut v: Vec<(&String, &usize)> = nodes.iter().collect();
            v.sort_by_key(|x| *x.1);
            v.into_iter().map(|x| x.0).collect()
        };
        for e in &mut g.edges {
            let (a, b) = (ids[e.a], ids[e.b]);
            let none = Vec::new();
            let (ra, rb) = (
                routes.get(a).unwrap_or(&none),
                routes.get(b).unwrap_or(&none),
            );
            let both: Vec<String> = ra.iter().filter(|r| rb.contains(r)).cloned().collect();
            e.routes = if both.is_empty() {
                let mut all: Vec<String> = ra.iter().chain(rb).cloned().collect();
                all.sort();
                all.dedup();
                all
            } else {
                both
            };
            e.along = views.iter().find(|v| v.consecutive(a, b)).map(|v| {
                let pos = |id: &str| v.structures.iter().position(|s| s == id);
                (v.id.clone(), pos(a) < pos(b))
            });
        }
        g
    }

    /// Adds `other`, the graph of a project line displayed (`name`): with
    /// `envelope`, its nodes at the place of one of this graph's (within
    /// 1 mm in plan) taken as that node and a span between two such nodes
    /// added to the edge there, its wires with this one's (one span of
    /// both: one envelope), its routes added as they are; else every node
    /// and span its own, its routes named "`name`: route" (`name` without
    /// one) and away from the alignments (its lines are its own, not along
    /// the working line's alignments in the profile).
    pub fn add(&mut self, other: Graph, envelope: bool, name: &str) {
        let place = |p: [f64; 2]| {
            (
                (p[0] * 1000.0).round() as i64,
                (p[1] * 1000.0).round() as i64,
            )
        };
        let mut here: HashMap<(i64, i64), usize> = HashMap::new();
        if envelope {
            for (k, p) in self.nodes.iter().enumerate() {
                here.entry(place(*p)).or_insert(k);
            }
        }
        let map: Vec<usize> = other
            .nodes
            .iter()
            .zip(other.joints)
            .map(|(p, joints)| {
                let k = match here.get(&place(*p)) {
                    Some(&k) => k,
                    None => {
                        self.nodes.push(*p);
                        self.joints.push(Vec::new());
                        let k = self.nodes.len() - 1;
                        if envelope {
                            here.insert(place(*p), k);
                        }
                        k
                    }
                };
                self.joints[k].extend(joints);
                k
            })
            .collect();
        let mut spans: HashMap<(usize, usize), usize> = HashMap::new();
        if envelope {
            for (k, e) in self.edges.iter().enumerate() {
                spans.entry((e.a.min(e.b), e.a.max(e.b))).or_insert(k);
            }
        }
        for e in other.edges {
            let (a, b) = (map[e.a], map[e.b]);
            let routes: Vec<String> = if envelope {
                e.routes
            } else if e.routes.is_empty() {
                vec![name.to_owned()]
            } else {
                e.routes.iter().map(|r| format!("{name}: {r}")).collect()
            };
            match spans.get(&(a.min(b), a.max(b))) {
                Some(&k) => {
                    let edge = &mut self.edges[k];
                    edge.sources.extend(e.sources);
                    for r in routes {
                        if !edge.routes.contains(&r) {
                            edge.routes.push(r);
                        }
                    }
                    if edge.along.is_none() {
                        edge.along = e.along;
                    }
                }
                None => {
                    self.edges.push(Edge {
                        a,
                        b,
                        sources: e.sources,
                        routes,
                        along: e.along.filter(|_| envelope),
                    });
                    if envelope {
                        spans.insert((a.min(b), a.max(b)), self.edges.len() - 1);
                    }
                }
            }
        }
    }

    /// Every source of `edges` and their nodes' joints.
    fn sources_of(&self, edges: &[usize]) -> Vec<Seg> {
        let mut nodes = BTreeSet::new();
        let mut out = Vec::new();
        for &e in edges {
            let edge = &self.edges[e];
            out.extend_from_slice(&edge.sources);
            nodes.insert(edge.a);
            nodes.insert(edge.b);
        }
        for k in nodes {
            out.extend_from_slice(&self.joints[k]);
        }
        out
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn len(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// Distance in plan from `p` to the segment `s`.
fn point_segment(p: [f64; 2], s: [[f64; 2]; 2]) -> f64 {
    let e = sub(s[1], s[0]);
    let l2 = dot(e, e);
    let t = if l2 > 0.0 {
        (dot(sub(p, s[0]), e) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    len(sub(p, [s[0][0] + e[0] * t, s[0][1] + e[1] * t]))
}

/// Whether two segments cross (or touch); where along `a` (0–1).
fn crossing(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> Option<f64> {
    let r = sub(a[1], a[0]);
    let s = sub(b[1], b[0]);
    let den = cross(r, s);
    if den.abs() < 1e-12 * (len(r) * len(s)).max(1e-300) {
        return None;
    }
    let q = sub(b[0], a[0]);
    let t = cross(q, s) / den;
    let u = cross(q, r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// The largest `t` with the point `p0 + t·n` (`n` a unit vector) within
/// `d` of the segment `s`: the far end of the line's piece inside the
/// segment's capsule; none when the line passes farther than `d`.
pub fn farthest_within(p0: [f64; 2], n: [f64; 2], s: [[f64; 2]; 2], d: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut take = |t: f64| best = Some(best.map_or(t, |b: f64| b.max(t)));
    // The discs at its ends.
    for q in s {
        let w = sub(q, p0);
        let along = dot(w, n);
        let off2 = dot(w, w) - along * along;
        if off2 <= d * d {
            take(along + (d * d - off2).max(0.0).sqrt());
        }
    }
    // The band along it: 0 ≤ e·(p − q0) ≤ ℓ, |m·(p − q0)| ≤ d.
    let e = sub(s[1], s[0]);
    let l = len(e);
    if l > 0.0 {
        let e = [e[0] / l, e[1] / l];
        let m = [-e[1], e[0]];
        let w = sub(p0, s[0]);
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for (c0, c1, min, max) in [
            (dot(e, w), dot(e, n), 0.0, l),
            (dot(m, w), dot(m, n), -d, d),
        ] {
            // min ≤ c0 + t·c1 ≤ max.
            if c1.abs() < 1e-15 {
                if c0 < min || c0 > max {
                    return best;
                }
            } else {
                let (a, b) = ((min - c0) / c1, (max - c0) / c1);
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
            }
        }
        if lo <= hi && hi.is_finite() {
            take(hi);
        }
    }
    best
}

/// The sources of a span, by bins along its chord.
struct Binned<'a> {
    sources: Vec<&'a Seg>,
    bin: f64,
    bins: Vec<Vec<u32>>,
}

impl<'a> Binned<'a> {
    /// `sources` along the chord from `a` (direction `u`, `length` long):
    /// each in the bins within `d` of its extent along the chord.
    fn new(
        a: [f64; 2],
        u: [f64; 2],
        length: f64,
        sources: impl IntoIterator<Item = &'a Seg>,
        d: f64,
    ) -> Self {
        let bin = (d / 2.0).max(4.0);
        let n = ((length / bin).ceil() as usize).max(1);
        let mut bins = vec![Vec::new(); n];
        let mut kept = Vec::new();
        for s in sources {
            let a0 = dot(sub(s[0], a), u);
            let a1 = dot(sub(s[1], a), u);
            let (lo, hi) = (a0.min(a1) - d, a0.max(a1) + d);
            if hi < 0.0 || lo > length {
                continue;
            }
            let first = ((lo.max(0.0) / bin) as usize).min(n - 1);
            let last = ((hi.min(length) / bin) as usize).min(n - 1);
            for b in &mut bins[first..=last] {
                b.push(kept.len() as u32);
            }
            kept.push(s);
        }
        Self {
            sources: kept,
            bin,
            bins,
        }
    }

    /// Those that may lie within the distance of the cross line at `a`
    /// along the chord.
    fn at(&self, a: f64) -> impl Iterator<Item = &'a Seg> + '_ {
        let b = ((a.max(0.0) / self.bin) as usize).min(self.bins.len() - 1);
        self.bins[b].iter().map(|&i| self.sources[i as usize])
    }
}

/// The farthest point out (largest `t`) along `p0 + t·n` within `d` of
/// any of `sources`.
fn reach<'a>(
    p0: [f64; 2],
    n: [f64; 2],
    sources: impl IntoIterator<Item = &'a Seg>,
    d: f64,
) -> Option<f64> {
    sources
        .into_iter()
        .filter_map(|s| farthest_within(p0, n, *s, d))
        .reduce(f64::max)
}

/// The tag of a point of a line beside no alignment (round a structure at
/// an end, or beside a span away from the alignments).
pub const NO_SIDE: u32 = u32::MAX;

/// A piece of a line in plan: its points, each point's tag (the half
/// span it lies beside: `2·edge` on the left of the edge from `a` to `b`,
/// `2·edge + 1` on its right; [`NO_SIDE`] round an end), and whether it
/// is closed (its last point its first).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Piece {
    /// The points.
    pub points: Vec<[f64; 2]>,
    /// Each point's tag.
    pub tags: Vec<u32>,
    /// Closed: round a whole part of the line.
    pub closed: bool,
}

/// The direction of half span `h` (`2·edge`: from `a` to `b`; + 1: back)
/// in `graph`: (from, to).
fn ends(graph: &Graph, h: usize) -> (usize, usize) {
    let e = &graph.edges[h / 2];
    if h.is_multiple_of(2) {
        (e.a, e.b)
    } else {
        (e.b, e.a)
    }
}

/// The lines round the spans `edges` of `graph`, `d` out from what they
/// hold: the outline of the band within `d` of every wire and jumper,
/// on both sides of every span, joined at the structures, closed round
/// the ends.
///
/// Each span's sides (left looking from `a` to `b`, and back), across
/// its chord every [`STEP`] at most: the farthest point out within `d`
/// of its wires (and the jumpers at its ends; [`farthest_within`]). The
/// sides are followed round the graph as its faces are, in turn about
/// each structure (the next span clockwise): where the line turns away
/// from its side the cross direction turns about the structure in steps
/// of at most 5° (a round outer corner, from the wires of both spans and
/// the jumpers there); at an end (a structure with one span) the line
/// goes on round it from one side to the other, 180° in steps of 5°, at
/// `d` beyond the wires and jumpers ending there (their points within
/// `d` along the span of the farthest towards the end: the outline of
/// their convex hull widened by `d` — round about a single wire's end,
/// straight across between several; a span's first or last points with
/// no wire within `d` left out there); a structure only holding
/// jumpers, all round. Whatever lies within `d` of a wire or jumper of
/// `edges` (beyond a tolerance of 0.2 % of `d`: an inner corner, a
/// branch, another part of the line beside or across) is cut off
/// ([`clip`]), the pieces left joined end to start where they cut each
/// other off ([`stitch`]), and any loop a line still makes cut off
/// ([`without_loops_tagged`]): the outer envelope. Pieces stay open
/// where a span holds no wire within `d` of its chord.
pub fn outline(graph: &Graph, edges: &[usize], d: f64) -> Vec<Piece> {
    let mut joints = graph.joints.clone();
    // The spans with a length (others: what they hold, at their node).
    let mut used = Vec::new();
    let mut touched = BTreeSet::new();
    for &e in edges {
        let edge = &graph.edges[e];
        touched.insert(edge.a);
        touched.insert(edge.b);
        if edge.sources.is_empty() {
            continue;
        }
        let (a, b) = (graph.nodes[edge.a], graph.nodes[edge.b]);
        if len(sub(b, a)) > 1e-9 {
            used.push(e);
        } else {
            joints[edge.a].extend_from_slice(&edge.sources);
        }
    }
    // The half spans out of each node, by angle (anticlockwise).
    let mut out: Vec<Vec<(f64, usize)>> = vec![Vec::new(); graph.nodes.len()];
    for &e in &used {
        for h in [2 * e, 2 * e + 1] {
            let (i, j) = ends(graph, h);
            let u = sub(graph.nodes[j], graph.nodes[i]);
            out[i].push((u[1].atan2(u[0]), h));
        }
    }
    for o in &mut out {
        o.sort_by(|x, y| x.0.total_cmp(&y.0));
    }
    // After half span h (into its end v): the next one clockwise about v
    // from h back.
    let next = |h: usize| {
        let (_, v) = ends(graph, h);
        let o = &out[v];
        let back = h ^ 1;
        let i = o.iter().position(|x| x.1 == back).expect("back out of v");
        o[(i + o.len() - 1) % o.len()].1
    };
    let dir = |h: usize| {
        let (i, j) = ends(graph, h);
        let u = sub(graph.nodes[j], graph.nodes[i]);
        let l = len(u);
        [u[0] / l, u[1] / l]
    };
    let left = |u: [f64; 2]| [-u[1], u[0]];
    let sources = |h: usize| -> &Vec<Seg> { &graph.edges[h / 2].sources };
    let mut pieces = Vec::new();
    let mut seen = vec![false; 2 * graph.edges.len()];
    for &e0 in &used {
        for h0 in [2 * e0, 2 * e0 + 1] {
            if seen[h0] {
                continue;
            }
            // One face: its points, None where it breaks.
            let mut raw: Vec<Option<([f64; 2], u32)>> = Vec::new();
            let mut h = h0;
            // Coming round an end: the span's first points with no wire
            // within d left out (the end's outline joins its first).
            let mut after_end = out[ends(graph, h0).0].len() == 1;
            loop {
                seen[h] = true;
                let (i, j) = ends(graph, h);
                let (a, v) = (graph.nodes[i], graph.nodes[j]);
                let u = dir(h);
                let n = left(u);
                let length = len(sub(v, a));
                let binned = Binned::new(
                    a,
                    u,
                    length,
                    sources(h).iter().chain(&joints[i]).chain(&joints[j]),
                    d,
                );
                let steps = ((length / STEP).ceil() as usize).max(1);
                let begun = raw.len();
                for k in 0..=steps {
                    let s = length * k as f64 / steps as f64;
                    let p0 = [a[0] + u[0] * s, a[1] + u[1] * s];
                    let t = reach(p0, n, binned.at(s), d);
                    if t.is_none() && after_end && raw.len() == begun {
                        continue;
                    }
                    raw.push(t.map(|t| ([p0[0] + n[0] * t, p0[1] + n[1] * t], h as u32)));
                }
                let h1 = next(h);
                after_end = h1 == h ^ 1;
                if after_end {
                    // Its last points with no wire within d left out too.
                    while raw.len() > begun && raw.last() == Some(&None) {
                        raw.pop();
                    }
                    // An end: round it, 180° clockwise, about what reaches
                    // farthest towards it (within d of the farthest).
                    let all: Vec<[f64; 2]> = sources(h)
                        .iter()
                        .chain(&joints[j])
                        .flatten()
                        .copied()
                        .collect();
                    let reach = all
                        .iter()
                        .map(|q| dot(sub(*q, v), u))
                        .fold(f64::NEG_INFINITY, f64::max);
                    let near: Vec<[f64; 2]> = all
                        .into_iter()
                        .filter(|q| dot(sub(*q, v), u) >= reach - d)
                        .collect();
                    if near.is_empty() {
                        raw.push(None);
                    }
                    let parts = (180.0 / FAN_STEP_DEG).ceil() as usize;
                    for k in 1..parts {
                        let ang = -std::f64::consts::PI * k as f64 / parts as f64;
                        if let Some(p) = support(&near, rotate(n, ang), d) {
                            raw.push(Some((p, NO_SIDE)));
                        }
                    }
                } else {
                    let n1 = left(dir(h1));
                    let turn = cross(n, n1).atan2(dot(n, n1));
                    if turn < 0.0 {
                        // Away from this side: round the outer corner.
                        let around: Vec<&Seg> = sources(h)
                            .iter()
                            .chain(sources(h1))
                            .chain(&joints[j])
                            .collect();
                        let parts =
                            ((turn.abs().to_degrees() / FAN_STEP_DEG).ceil() as usize).max(1);
                        for k in 1..parts {
                            let r = rotate(n, turn * k as f64 / parts as f64);
                            let t = reach(v, r, around.iter().copied(), d);
                            raw.push(t.map(|t| ([v[0] + r[0] * t, v[1] + r[1] * t], h as u32)));
                        }
                    }
                }
                h = h1;
                if h == h0 {
                    break;
                }
            }
            pieces.extend(face_pieces(raw));
        }
    }
    // Structures holding jumpers only: all round.
    for k in touched {
        if !out[k].is_empty() || joints[k].is_empty() {
            continue;
        }
        let near: Vec<[f64; 2]> = joints[k].iter().flatten().copied().collect();
        let parts = (360.0 / FAN_STEP_DEG).ceil() as usize;
        let mut points: Vec<[f64; 2]> = (0..parts)
            .filter_map(|i| {
                let ang = -std::f64::consts::TAU * i as f64 / parts as f64;
                support(&near, rotate([0.0, 1.0], ang), d)
            })
            .collect();
        if points.len() > 2 {
            points.push(points[0]);
            pieces.push(Piece {
                tags: vec![NO_SIDE; points.len()],
                points,
                closed: true,
            });
        }
    }
    let all = graph.sources_of(edges);
    let near = Near::new(&all, d);
    let tol = 0.002 * d + 1e-6;
    let clipped = pieces
        .iter()
        .flat_map(|p| clip(p, |q| near.inside(q, d - tol)))
        .collect();
    stitch(clipped, (0.5 * d).clamp(0.05, 1.0))
        .into_iter()
        .map(|p| {
            let (points, tags) = without_loops_tagged(&p.points, &p.tags);
            Piece {
                closed: p.closed && points.first() == points.last(),
                points,
                tags,
            }
        })
        .collect()
}

/// `v` turned by `ang` (radians, anticlockwise).
fn rotate(v: [f64; 2], ang: f64) -> [f64; 2] {
    let (s, c) = ang.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

/// The point `d` beyond the farthest of `points` in direction `dir` (a
/// unit vector): on the outline of their convex hull widened by `d`.
fn support(points: &[[f64; 2]], dir: [f64; 2], d: f64) -> Option<[f64; 2]> {
    let q = points
        .iter()
        .max_by(|a, b| dot(**a, dir).total_cmp(&dot(**b, dir)))?;
    Some([q[0] + dir[0] * d, q[1] + dir[1] * d])
}

/// The points of one face round the graph (closed, None where it
/// breaks) as pieces without loops: one closed piece when unbroken
/// (begun at its leftmost point, on the outer envelope), else a piece
/// between each two breaks.
fn face_pieces(raw: Vec<Option<([f64; 2], u32)>>) -> Vec<Piece> {
    let mut runs: Vec<Vec<([f64; 2], u32)>> = Vec::new();
    let closed = raw.iter().all(Option::is_some);
    if closed {
        let mut pts: Vec<([f64; 2], u32)> = raw.into_iter().flatten().collect();
        let first = pts
            .iter()
            .enumerate()
            .min_by(|a, b| {
                a.1.0[0]
                    .total_cmp(&b.1.0[0])
                    .then(a.1.0[1].total_cmp(&b.1.0[1]))
            })
            .map_or(0, |x| x.0);
        pts.rotate_left(first);
        if let Some(&p) = pts.first() {
            pts.push(p);
        }
        runs.push(pts);
    } else {
        let start = raw.iter().position(Option::is_none).unwrap_or(0);
        let mut current = Vec::new();
        for p in raw[start..].iter().chain(&raw[..start]) {
            match p {
                Some(p) => current.push(*p),
                None => runs.push(std::mem::take(&mut current)),
            }
        }
        runs.push(current);
    }
    runs.into_iter()
        .filter_map(|run| {
            let mut points: Vec<[f64; 2]> = Vec::with_capacity(run.len());
            let mut tags = Vec::with_capacity(run.len());
            for (p, t) in run {
                if points
                    .last()
                    .is_none_or(|q: &[f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-6)
                {
                    points.push(p);
                    tags.push(t);
                }
            }
            if closed && points.len() > 1 {
                // The first point again at the end, removed as a repeat
                // when the face is a single point.
                let p = points[0];
                if points.last() != Some(&p) {
                    points.push(p);
                    tags.push(tags[0]);
                }
            }
            (points.len() > 1).then(|| Piece {
                closed: closed && points.len() > 3 && points.first() == points.last(),
                points,
                tags,
            })
        })
        .collect()
}

/// Segments by cells of a grid, for the distance to the nearest.
struct Near<'a> {
    segs: &'a [Seg],
    cell: f64,
    grid: HashMap<(i64, i64), Vec<u32>>,
}

impl<'a> Near<'a> {
    /// `segs`, for distances up to about `d`.
    fn new(segs: &'a [Seg], d: f64) -> Self {
        let cell = d.max(2.0);
        let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (k, s) in segs.iter().enumerate() {
            let c0 = Self::cell_of(cell, s[0]);
            let c1 = Self::cell_of(cell, s[1]);
            for i in c0.0.min(c1.0)..=c0.0.max(c1.0) {
                for j in c0.1.min(c1.1)..=c0.1.max(c1.1) {
                    grid.entry((i, j)).or_default().push(k as u32);
                }
            }
        }
        Self { segs, cell, grid }
    }

    fn cell_of(cell: f64, p: [f64; 2]) -> (i64, i64) {
        ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64)
    }

    /// Whether `p` lies nearer than `r` (at most about the `d` given) to a
    /// segment.
    fn inside(&self, p: [f64; 2], r: f64) -> bool {
        if r <= 0.0 {
            return false;
        }
        let lo = Self::cell_of(self.cell, [p[0] - r, p[1] - r]);
        let hi = Self::cell_of(self.cell, [p[0] + r, p[1] + r]);
        for i in lo.0..=hi.0 {
            for j in lo.1..=hi.1 {
                for &k in self.grid.get(&(i, j)).map_or(&[][..], Vec::as_slice) {
                    if point_segment(p, self.segs[k as usize]) < r {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// `piece` without what lies `inside`: the pieces left, each cut where
/// it goes in (found within a micrometre along the segment), the cut
/// points tagged as the segment's start.
fn clip(piece: &Piece, inside: impl Fn([f64; 2]) -> bool) -> Vec<Piece> {
    let pts = &piece.points;
    let mut ins: Vec<bool> = pts.iter().map(|p| inside(*p)).collect();
    if !ins.iter().any(|x| *x) {
        return vec![piece.clone()];
    }
    if piece.closed {
        let n = ins.len();
        ins[n - 1] = ins[0];
    }
    if ins.iter().all(|x| *x) {
        return Vec::new();
    }
    // Where along a → b (a outside, b inside) it goes in.
    let cut = |a: [f64; 2], b: [f64; 2]| {
        let (mut lo, mut hi) = (0.0, 1.0);
        let l = len(sub(b, a));
        while (hi - lo) * l > 1e-6 {
            let m = (lo + hi) / 2.0;
            if inside([a[0] + (b[0] - a[0]) * m, a[1] + (b[1] - a[1]) * m]) {
                hi = m;
            } else {
                lo = m;
            }
        }
        [a[0] + (b[0] - a[0]) * lo, a[1] + (b[1] - a[1]) * lo]
    };
    // A closed piece: begun at a point inside.
    let order: Vec<usize> = if piece.closed {
        let first = ins.iter().position(|x| *x).unwrap_or(0);
        let n = pts.len() - 1;
        (0..=n).map(|k| (first + k) % n).collect()
    } else {
        (0..pts.len()).collect()
    };
    let mut out = Vec::new();
    let mut current = Piece::default();
    for w in order.windows(2) {
        let (i, j) = (w[0], w[1]);
        match (ins[i], ins[j]) {
            (false, false) => {
                if current.points.is_empty() {
                    current.points.push(pts[i]);
                    current.tags.push(piece.tags[i]);
                }
                current.points.push(pts[j]);
                current.tags.push(piece.tags[j]);
            }
            (false, true) => {
                if current.points.is_empty() {
                    current.points.push(pts[i]);
                    current.tags.push(piece.tags[i]);
                }
                current.points.push(cut(pts[i], pts[j]));
                current.tags.push(piece.tags[i]);
                out.push(std::mem::take(&mut current));
            }
            (true, false) => {
                current.points.push(cut(pts[j], pts[i]));
                current.tags.push(piece.tags[i]);
                current.points.push(pts[j]);
                current.tags.push(piece.tags[j]);
            }
            (true, true) => {}
        }
    }
    out.push(current);
    for p in &mut out {
        let mut keep = Vec::with_capacity(p.points.len());
        let mut tags = Vec::with_capacity(p.points.len());
        for (q, t) in p.points.iter().zip(&p.tags) {
            if keep
                .last()
                .is_none_or(|r: &[f64; 2]| (r[0] - q[0]).hypot(r[1] - q[1]) > 1e-9)
            {
                keep.push(*q);
                tags.push(*t);
            }
        }
        p.points = keep;
        p.tags = tags;
    }
    out.retain(|p| p.points.len() > 1);
    out
}

/// Where the line through `a0`, `a1` meets the line through `b0`, `b1`
/// (none: parallel).
fn meet(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> Option<[f64; 2]> {
    let (r, q) = (sub(a1, a0), sub(b1, b0));
    let den = cross(r, q);
    if den.abs() < 1e-12 * (len(r) * len(q)).max(1e-300) {
        return None;
    }
    let t = cross(sub(b0, a0), q) / den;
    Some([a0[0] + r[0] * t, a0[1] + r[1] * t])
}

/// Joins the end of `p` (its last segment) to `q` (its first segment
/// from `q[0]`; `tags` its points' tags): where the two segments' lines
/// meet when that is within `within` of both ends (two lines that cut
/// each other off, joined where they cross: no step back and forth),
/// else straight across.
fn join(p: &mut Piece, q: &[[f64; 2]], tags: &[u32], within: f64) {
    let end = *p.points.last().expect("a piece has points");
    let start = q[0];
    let n = p.points.len();
    let x = (n > 1 && q.len() > 1)
        .then(|| meet(p.points[n - 2], end, start, q[1]))
        .flatten()
        .filter(|x| len(sub(*x, end)) <= within && len(sub(*x, start)) <= within);
    match x {
        Some(x) => {
            *p.points.last_mut().expect("a piece has points") = x;
            p.points.extend_from_slice(&q[1..]);
            p.tags.extend_from_slice(&tags[1..]);
        }
        None => {
            let skip = usize::from(start == end);
            p.points.extend_from_slice(&q[skip..]);
            p.tags.extend_from_slice(&tags[skip..]);
        }
    }
}

/// `pieces` joined where one ends within `within` of where another
/// begins (in the order found; [`join`]), closed where a piece comes back
/// to its start.
fn stitch(mut pieces: Vec<Piece>, within: f64) -> Vec<Piece> {
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) <= within;
    let mut done = Vec::new();
    while let Some(mut p) = pieces.pop() {
        if p.closed {
            done.push(p);
            continue;
        }
        loop {
            let end = *p.points.last().expect("a piece has points");
            if p.points.len() > 3 && near(end, p.points[0]) {
                // Joined to its own start, which is then left out: it
                // begins and ends at its second point.
                let (head, tags) = (p.points[..2].to_vec(), p.tags[..2].to_vec());
                join(&mut p, &head, &tags, within);
                p.points.remove(0);
                p.tags.remove(0);
                p.closed = true;
                break;
            }
            // The nearest start.
            let Some(k) = pieces
                .iter()
                .enumerate()
                .filter(|(_, q)| !q.closed && near(end, q.points[0]))
                .min_by(|a, b| {
                    len(sub(a.1.points[0], end)).total_cmp(&len(sub(b.1.points[0], end)))
                })
                .map(|x| x.0)
            else {
                break;
            };
            let q = pieces.swap_remove(k);
            join(&mut p, &q.points, &q.tags, within);
        }
        done.push(p);
    }
    done
}

/// `line` without the loops it makes where it crosses itself: from each
/// segment, the next crossing farthest along is joined directly, so the
/// result never crosses itself; each point keeping its tag (a crossing
/// point takes the tag of the segment the line goes on along). A closed
/// `line` (its last point its first) stays closed.
pub fn without_loops_tagged(line: &[[f64; 2]], tags: &[u32]) -> (Vec<[f64; 2]>, Vec<u32>) {
    // Repeated points out (segments of no length).
    let (line, tags) = {
        let mut points: Vec<[f64; 2]> = Vec::with_capacity(line.len());
        let mut kept = Vec::with_capacity(line.len());
        for (k, p) in line.iter().enumerate() {
            let last = k + 1 == line.len();
            match points.last_mut() {
                Some(q) if (q[0] - p[0]).hypot(q[1] - p[1]) < 1e-9 => {
                    // The last point kept exactly (a closed line's end).
                    if last {
                        *q = *p;
                    }
                }
                _ => {
                    points.push(*p);
                    kept.push(tags[k]);
                }
            }
        }
        (points, kept)
    };
    let (line, tags) = (&line[..], &tags[..]);
    let n = line.len();
    if n < 4 {
        return (line.to_vec(), tags.to_vec());
    }
    let closed = line[0] == line[n - 1];
    // A grid of the segments.
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let mut total = 0.0;
    for (k, p) in line.iter().enumerate() {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
        if k > 0 {
            total += len(sub(*p, line[k - 1]));
        }
    }
    let cell = (total / n as f64 * 4.0).max(1.0);
    let cols = (((hi[0] - lo[0]) / cell) as usize + 1).min(4096);
    let rows = (((hi[1] - lo[1]) / cell) as usize + 1).min(4096);
    let cell_of = |p: [f64; 2]| {
        (
            (((p[0] - lo[0]) / cell) as usize).min(cols - 1),
            (((p[1] - lo[1]) / cell) as usize).min(rows - 1),
        )
    };
    let mut grid: HashMap<(usize, usize), Vec<usize>> = Default::default();
    for k in 0..n - 1 {
        let (a, b) = (cell_of(line[k]), cell_of(line[k + 1]));
        for i in a.0.min(b.0)..=a.0.max(b.0) {
            for j in a.1.min(b.1)..=a.1.max(b.1) {
                grid.entry((i, j)).or_default().push(k);
            }
        }
    }
    let mut out = vec![line[0]];
    let mut out_tags = vec![tags[0]];
    let mut k = 0;
    let mut start = line[0];
    while k < n - 1 {
        let seg = [start, line[k + 1]];
        let (a, b) = (cell_of(seg[0]), cell_of(seg[1]));
        let mut best: Option<(usize, f64, [f64; 2])> = None;
        for i in a.0.min(b.0)..=a.0.max(b.0) {
            for j in a.1.min(b.1)..=a.1.max(b.1) {
                for &m in grid.get(&(i, j)).map_or(&[][..], Vec::as_slice) {
                    if m < k + 2 || best.is_some_and(|x| x.0 >= m) {
                        continue;
                    }
                    // A closed line's last segment meets its first at
                    // the start: not a crossing.
                    if closed && k == 0 && m == n - 2 {
                        continue;
                    }
                    let other = [line[m], line[m + 1]];
                    if let Some(t) = crossing(seg, other) {
                        // Nor anything touching it there.
                        if closed && k == 0 && t < 1e-12 {
                            continue;
                        }
                        let x = [
                            seg[0][0] + (seg[1][0] - seg[0][0]) * t,
                            seg[0][1] + (seg[1][1] - seg[0][1]) * t,
                        ];
                        best = Some((m, t, x));
                    }
                }
            }
        }
        match best {
            Some((m, _, x)) => {
                out.push(x);
                out_tags.push(tags[m]);
                start = x;
                k = m;
            }
            None => {
                out.push(line[k + 1]);
                out_tags.push(tags[k + 1]);
                k += 1;
                start = line[k];
            }
        }
    }
    // Repeats out.
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(out.len());
    let mut kept = Vec::with_capacity(out.len());
    for (p, t) in out.into_iter().zip(out_tags) {
        if points
            .last()
            .is_none_or(|q: &[f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]) >= 1e-9)
        {
            points.push(p);
            kept.push(t);
        }
    }
    (points, kept)
}

/// [`drape_counted`] without the gaps (tests).
#[cfg(test)]
pub fn drape(plan: &[[f64; 2]], ground: Option<&Terrain>, axis: &[[f64; 3]]) -> Vec<Vec<[f64; 3]>> {
    drape_counted(plan, ground, axis).0
}

/// [`drape_tagged`] along the alignment `axis` (tests).
#[cfg(test)]
pub fn drape_counted(
    plan: &[[f64; 2]],
    ground: Option<&Terrain>,
    axis: &[[f64; 3]],
) -> (Vec<Vec<[f64; 3]>>, usize) {
    let (runs, _, gaps) = drape_tagged(plan, &vec![NO_SIDE; plan.len()], ground, &[axis]);
    (runs, gaps)
}

/// Pieces on the ground, each point's tag, and the stretches without
/// ground ([`drape_tagged`]).
pub type Draped = (Vec<Vec<[f64; 3]>>, Vec<Vec<u32>>, usize);

/// `plan` on the ground: on `ground`, a point where it crosses each edge
/// of its triangles (the ground is flat between them), broken where it
/// leaves the surface; without one, every [`STEP`] at the height of the
/// nearest of the alignments `axes` (its points' heights, linear along
/// it) where it is beside it. Each point with the tag of the point of
/// `plan` its segment begins at; with the number of stretches of `plan`
/// that have no ground under them (each counted once, however many
/// segments it spans; none without a ground).
pub fn drape_tagged(
    plan: &[[f64; 2]],
    tags: &[u32],
    ground: Option<&Terrain>,
    axes: &[&[[f64; 3]]],
) -> Draped {
    let mut gaps = 0;
    // In a stretch without ground (counted already).
    let mut off = false;
    let mut miss = |off: &mut bool| {
        if !*off {
            gaps += 1;
            *off = true;
        }
    };
    type Run = (Vec<[f64; 3]>, Vec<u32>);
    let mut runs: Vec<Run> = Vec::new();
    let mut current: Run = (Vec::new(), Vec::new());
    let flush = |current: &mut Run, runs: &mut Vec<Run>| {
        if current.0.len() > 1 {
            runs.push(std::mem::take(current));
        } else {
            current.0.clear();
            current.1.clear();
        }
    };
    let push = |current: &mut Run, p: [f64; 3], tag: u32| {
        if current
            .0
            .last()
            .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-9)
        {
            current.0.push(p);
            current.1.push(tag);
        }
    };
    match ground {
        Some(g) => {
            for (k, w) in plan.windows(2).enumerate() {
                // A repeated point: nothing between.
                if len(sub(w[1], w[0])) < 1e-9 {
                    continue;
                }
                let tag = tags[k];
                let pieces = g.profile(w[0], w[1]);
                if pieces.is_empty() {
                    flush(&mut current, &mut runs);
                    miss(&mut off);
                    continue;
                }
                for (i, run) in pieces.iter().enumerate() {
                    let joins = i == 0 && run.first().is_some_and(|p| p.t < 1e-9);
                    if !joins {
                        flush(&mut current, &mut runs);
                        miss(&mut off);
                    }
                    for p in run {
                        push(&mut current, p.point, tag);
                    }
                    if !run.is_empty() {
                        off = false;
                    }
                }
                if pieces
                    .last()
                    .and_then(|r| r.last())
                    .is_none_or(|p| p.t < 1.0 - 1e-9)
                {
                    flush(&mut current, &mut runs);
                    miss(&mut off);
                }
            }
        }
        None => {
            for (k, w) in plan.windows(2).enumerate() {
                let l = len(sub(w[1], w[0]));
                let steps = ((l / STEP).ceil() as usize).max(1);
                for i in 0..=steps {
                    let f = i as f64 / steps as f64;
                    let p = [
                        w[0][0] + (w[1][0] - w[0][0]) * f,
                        w[0][1] + (w[1][1] - w[0][1]) * f,
                    ];
                    push(&mut current, [p[0], p[1], axes_height(axes, p)], tags[k]);
                }
            }
        }
    }
    flush(&mut current, &mut runs);
    let (points, tags) = runs.into_iter().unzip();
    (points, tags, gaps)
}

/// The height at `p` of the nearest of the alignments `axes`
/// ([`axis_height`]; none: 0).
fn axes_height(axes: &[&[[f64; 3]]], p: [f64; 2]) -> f64 {
    axes.iter()
        .filter(|a| !a.is_empty())
        .map(|a| {
            let near = a
                .windows(2)
                .map(|w| point_segment(p, [[w[0][0], w[0][1]], [w[1][0], w[1][1]]]))
                .fold(f64::INFINITY, f64::min);
            (near, axis_height(a, p))
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map_or(0.0, |x| x.1)
}

/// The height of the alignment `axis` at the station of `p` (linear
/// between its points).
fn axis_height(axis: &[[f64; 3]], p: [f64; 2]) -> f64 {
    let (st, _) = ohl_line::route::project(axis, [p[0], p[1], 0.0]);
    let mut start = 0.0;
    for w in axis.windows(2) {
        let l = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if st <= start + l || l == 0.0 {
            let f = if l > 0.0 {
                ((st - start) / l).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return w[0][2] + (w[1][2] - w[0][2]) * f;
        }
        start += l;
    }
    axis.last().map_or(0.0, |p| p[2])
}

/// The lines in the 3D view: each piece a little above the ground (so it
/// is not hidden in it), dashed when chosen.
pub fn scene(origin: [f64; 3], drawn: Option<&Drawn>, settings: &Settings) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let Some(d) = drawn else {
        return s;
    };
    let [r, g, b] = settings.color;
    for l in &d.lines {
        for run in &l.runs {
            let lifted: Vec<[f64; 3]> = run.iter().map(|p| [p[0], p[1], p[2] + 0.2]).collect();
            if settings.dashed {
                for dash in crate::wire_style::dashes(&lifted, &[6.0, 4.0], 1.0) {
                    s.polyline(&dash, [r, g, b, 255], settings.width);
                }
            } else {
                s.polyline(&lifted, [r, g, b, 255], settings.width);
            }
        }
    }
    s
}

/// The lines in the plan view (those in view).
pub fn paint_plan(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    drawn: Option<&Drawn>,
    settings: &Settings,
) {
    let Some(d) = drawn else {
        return;
    };
    let [r, g, b] = settings.color;
    let stroke = egui::Stroke::new(settings.width, egui::Color32::from_rgb(r, g, b));
    let near = rect.expand(50.0);
    for l in &d.lines {
        for run in &l.runs {
            let pts = crate::plan::thin_px(run.iter().map(|p| to_screen(*p)));
            if pts.len() < 2 || !pts.iter().any(|p| near.contains(*p)) {
                continue;
            }
            if settings.dashed {
                painter.extend(egui::Shape::dashed_line(&pts, stroke, 8.0, 5.0));
            } else {
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

/// The lines along the profile of an alignment drawn from station `at`:
/// (side, pieces of (station, height)) — the points beside its spans
/// (by their tags, [`Drawn::beside`]) and beside its axis (not beyond its
/// ends).
pub fn along(drawn: &Drawn, alignment: &str, at: f64) -> Vec<(Side, Vec<Vec<[f64; 2]>>)> {
    let Some((_, axis)) = drawn.axes.iter().find(|(id, _)| id == alignment) else {
        return Vec::new();
    };
    let mut out: Vec<(Side, Vec<Vec<[f64; 2]>>)> = Vec::new();
    let mut flush = |side: Option<Side>, piece: &mut Vec<[f64; 2]>| {
        if let Some(side) = side
            && piece.len() > 1
        {
            match out.iter_mut().find(|x| x.0 == side) {
                Some(x) => x.1.push(std::mem::take(piece)),
                None => out.push((side, vec![std::mem::take(piece)])),
            }
        }
        piece.clear();
    };
    for l in &drawn.lines {
        for (run, tags) in l.runs.iter().zip(&l.tags) {
            let mut side: Option<Side> = None;
            let mut piece: Vec<[f64; 2]> = Vec::new();
            for (p, tag) in run.iter().zip(tags) {
                let here = drawn
                    .side_of(*tag)
                    .filter(|(id, _)| *id == alignment)
                    .map(|x| x.1)
                    .filter(|_| {
                        let (st, off) = ohl_line::route::project(axis, *p);
                        let back = ohl_line::route::point_at(axis, st, off);
                        (back[0] - p[0]).hypot(back[1] - p[1]) < 0.5
                    });
                if here != side {
                    flush(side, &mut piece);
                    side = here;
                }
                if here.is_some() {
                    let (st, _) = ohl_line::route::project(axis, *p);
                    piece.push([at + st, p[2]]);
                }
            }
            flush(side, &mut piece);
        }
    }
    out
}

impl Drawn {
    /// The alignment and side the point of tag `tag` lies beside (none:
    /// round an end, or beside a span away from the alignments).
    pub fn side_of(&self, tag: u32) -> Option<(&str, Side)> {
        self.beside
            .get(tag as usize)?
            .as_ref()
            .map(|(id, side)| (id.as_str(), *side))
    }

    /// The points of the lines beside the alignment `alignment` on
    /// `side`, in plan and on the ground (tests).
    #[cfg(test)]
    pub fn beside_points<'a>(
        &'a self,
        alignment: &'a str,
        side: Side,
    ) -> impl Iterator<Item = [f64; 3]> + 'a {
        self.lines.iter().flat_map(move |l| {
            l.runs
                .iter()
                .zip(&l.tags)
                .flat_map(|(r, t)| r.iter().zip(t))
                .filter(move |(_, t)| self.side_of(**t) == Some((alignment, side)))
                .map(|(p, _)| *p)
        })
    }
}

/// Every route and branch of `line` (id, name): its routes in order,
/// then those only its structures give (no name).
pub fn routes(line: &ohl_line::Line) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = line
        .routes
        .iter()
        .map(|r| (r.id.clone(), r.name.clone()))
        .collect();
    let more: BTreeSet<String> = line.structure_routes().into_values().flatten().collect();
    for r in more {
        if !out.iter().any(|x| x.0 == r) {
            out.push((r, String::new()));
        }
    }
    out
}

/// The menu (View › Lines on the ground beside the line), `routes` every
/// route and branch of the line (id, name), `cases` the weather cases'
/// names, `shown` the case shown, `model_corridor` the
/// model's LiDAR corridor (m): whether the settings changed. `build` set when **Build a terrain surface from the LiDAR…**
/// is clicked (offered when neither a surface nor the LiDAR ground lies
/// under the lines and there is a point cloud, [`Drawn::offer_build`]).
#[allow(clippy::too_many_arguments)]
pub fn menu(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    routes: &[(String, String)],
    displayed: &str,
    projects: &[String],
    model_corridor: f64,
    drawn: Option<&Drawn>,
    build: &mut bool,
) -> bool {
    let before = settings.clone();
    ui.label(
        egui::RichText::new(
            "A line each side of the line, on the ground, round the weather cases and the \
             project lines displayed (yours only; display only)",
        )
        .small()
        .weak(),
    );
    ui.checkbox(&mut settings.on, "Show the lines");
    ui.horizontal(|ui| {
        ui.label("Distance");
        ui.add(
            egui::DragValue::new(&mut settings.distance)
                .range(0.0..=MAX_DISTANCE)
                .speed(0.5)
                .max_decimals(2)
                .suffix(" m"),
        );
    });
    corridor_field(ui, settings, model_corridor, drawn);
    ui.label(egui::RichText::new("Measured from").small().weak());
    ui.radio_value(
        &mut settings.from,
        From::OutermostConductor,
        "the outermost conductor (with its blowout)",
    )
    .on_hover_text(
        "In plan, every point of every wire and jumper in the weather cases displayed — swung \
         out by the wind from either side when a case has wind — on each side its own \
         outermost conductor: the envelope over all of them, and over the project lines \
         displayed",
    );
    ui.radio_value(
        &mut settings.from,
        From::Axis,
        "the line's axis (alignment)",
    );
    if settings.from == From::OutermostConductor {
        ui.label(
            egui::RichText::new(format!(
                "Weather cases: as displayed ({displayed}), both wind sides"
            ))
            .small()
            .weak(),
        )
        .on_hover_text(
            "The weather cases the views display (View › Display weather cases; a section \
             drawn in cases of its own, in those): change them there",
        );
    }
    ui.label(
        egui::RichText::new(if projects.is_empty() {
            "Project lines: none displayed (Versions menu, layer Other project lines)".to_owned()
        } else {
            format!("Project lines: as displayed ({})", projects.join(", "))
        })
        .small()
        .weak(),
    )
    .on_hover_text(
        "The project lines shown next to the line (Versions menu) count while the layer \
         \"Other project lines\" is shown, each as it is drawn",
    );
    if !routes.is_empty() {
        ui.label(
            egui::RichText::new("Routes and branches (the lines round those ticked)")
                .small()
                .weak(),
        );
        egui::ScrollArea::vertical()
            .id_salt("ground-lines-routes")
            .max_height(160.0)
            .show(ui, |ui| {
                for (id, name) in routes {
                    let mut on = !settings.hidden_routes.contains(id);
                    let text = if name.is_empty() || name == id {
                        id.clone()
                    } else {
                        format!("{name} ({id})")
                    };
                    if ui.checkbox(&mut on, text).changed() {
                        if on {
                            settings.hidden_routes.remove(id);
                        } else {
                            settings.hidden_routes.insert(id.clone());
                        }
                    }
                }
            });
    }
    if !routes.is_empty() || !projects.is_empty() {
        ui.radio_value(
            &mut settings.route_lines,
            RouteLines::Envelope,
            "One envelope for all routes and project lines",
        )
        .on_hover_text(
            "Where routes or project lines run side by side or cross, each side line is the \
             outer edge of all the routes ticked and the project lines displayed: the lines \
             never run inside another's band",
        );
        ui.radio_value(
            &mut settings.route_lines,
            RouteLines::Each,
            "Each route and project line its own lines",
        )
        .on_hover_text(
            "Each route ticked and each project line displayed has its own closed lines, \
             whatever lies beside it: where they run side by side or cross, their lines cross",
        );
    }
    ui.label(egui::RichText::new("Style").small().weak());
    ui.horizontal(|ui| {
        for c in COLORS {
            let col = egui::Color32::from_rgb(c[0], c[1], c[2]);
            let chosen = settings.color == c;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
            ui.painter().rect_filled(rect.shrink(2.0), 2.0, col);
            if chosen {
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.5, ui.visuals().strong_text_color()),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.on_hover_text("This colour").clicked() {
                settings.color = c;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Width");
        ui.add(
            egui::DragValue::new(&mut settings.width)
                .range(1.0..=6.0)
                .speed(0.1)
                .max_decimals(1)
                .suffix(" px"),
        );
        ui.checkbox(&mut settings.dashed, "Dashed");
    });
    ui.checkbox(
        &mut settings.profile,
        "In the profile too (right side near, left side far, dashed)",
    );
    if settings.on
        && let Some(d) = drawn
    {
        let within = d
            .corridor
            .map_or(String::new(), |w| format!(" within ±{w:.0} m"));
        ui.label(
            egui::RichText::new(format!("Ground: {}{within}", d.ground.label()))
                .small()
                .weak(),
        )
        .on_hover_text(
            "The active terrain surface, else the ground built from the LiDAR ground classes \
             of the whole LiDAR corridor (the lowest point of each 1 m cell, as the checks use \
             it; a wider corridor chosen here: a ground of the lines' own, the checks keep the \
             model's), else the heights of the alignment's points",
        );
        for n in &d.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        if let Some(why) = &d.offer_build {
            ui.label(
                egui::RichText::new(format!("No ground under the lines: {why}"))
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
            if ui
                .button("Build a terrain surface from the LiDAR…")
                .on_hover_text(
                    "Terrain › Terrain surfaces (TIN), Build from LiDAR: tick the classes of \
                     the ground and build; the surface becomes the ground and the lines are \
                     draped on it",
                )
                .clicked()
            {
                *build = true;
                ui.close();
            }
        }
    }
    *settings != before
}

/// The LiDAR corridor of the lines: from the model's (`model_corridor`,
/// none chosen) to [`MAX_CORRIDOR`]; only of use on the LiDAR ground.
fn corridor_field(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    model_corridor: f64,
    drawn: Option<&Drawn>,
) {
    let lo = model_corridor.clamp(0.0, MAX_CORRIDOR);
    let mut w = settings
        .lidar_corridor
        .filter(|w| w.is_finite())
        .map_or(model_corridor, |w| w.clamp(0.0, MAX_CORRIDOR))
        .max(lo);
    let on_tin = drawn.is_some_and(|d| matches!(d.ground, Ground::Surface(_)));
    ui.horizontal(|ui| {
        ui.label("LiDAR corridor ±");
        let r = ui
            .add_enabled(
                !on_tin,
                egui::DragValue::new(&mut w)
                    .range(lo..=MAX_CORRIDOR)
                    .speed(1.0)
                    .max_decimals(0)
                    .suffix(" m"),
            )
            .on_hover_text(CORRIDOR_HOVER)
            .on_disabled_hover_text(
                "The lines lie on the terrain surface: the LiDAR corridor is of no use to them",
            );
        if r.changed() {
            settings.lidar_corridor = (w > model_corridor).then_some(w);
        }
        if settings.lidar_corridor.is_some()
            && ui
                .small_button("Model's")
                .on_hover_text(format!(
                    "The model's LiDAR corridor (±{model_corridor:.0} m) and its ground"
                ))
                .clicked()
        {
            settings.lidar_corridor = None;
        }
    });
}

/// The hover text of the LiDAR corridor field.
pub const CORRIDOR_HOVER: &str = "Half width of the LiDAR ground the lines are draped on, m from \
    the spans' chords (as the model's corridor). By default the model's (60 m unless set \
    otherwise): its ground, nothing more read. Wider: a ground of the lines' own is built in \
    the background from the ground classes of the whole cloud (kept per width); the checks keep \
    the model's ground, their results do not change. At most 200 m: the ground's memory grows \
    with the width — at 200 m about 0.4 million 1 m cells and 90 MB per km of line, 3.3 times \
    the model's ground at 60 m, about a second per km to build (measured on synthetic clouds \
    of 5 and 12 million points) — while a line's corridor survey rarely holds ground points \
    farther out.";

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(z: f64) -> Terrain {
        Terrain::from_xyz(
            &[
                [-500.0, -500.0, z],
                [500.0, -500.0, z],
                [500.0, 500.0, z],
                [-500.0, 500.0, z],
            ],
            None,
        )
        .unwrap()
    }

    /// One span from `a` to `b` holding `wires`.
    fn span(a: [f64; 2], b: [f64; 2], wires: &[Seg]) -> Graph {
        Graph::path(vec![a, b], vec![wires.to_vec()], Vec::new())
    }

    /// The axis as its own graph (each leg holding its chord).
    fn axis_graph(axis: &[[f64; 2]]) -> Graph {
        let spans = axis.windows(2).map(|w| vec![[w[0], w[1]]]).collect();
        Graph::path(axis.to_vec(), spans, Vec::new())
    }

    /// The outline round every span of `g`.
    fn all(g: &Graph, d: f64) -> Vec<Piece> {
        let edges: Vec<usize> = (0..g.edges.len()).collect();
        outline(g, &edges, d)
    }

    /// No two segments of `line` cross (a closed line's first and last
    /// meet at its start).
    fn assert_simple(line: &[[f64; 2]], what: &str) {
        let n = line.len();
        let closed = line[0] == line[n - 1];
        for i in 0..n - 1 {
            for j in i + 2..n - 1 {
                if closed && i == 0 && j == n - 2 {
                    continue;
                }
                let c = crossing([line[i], line[i + 1]], [line[j], line[j + 1]]);
                assert!(c.is_none(), "{what}: {i} × {j}");
            }
        }
    }

    /// The nearest of `sources` to `p`, m.
    fn nearest(sources: &[Seg], p: [f64; 2]) -> f64 {
        sources
            .iter()
            .map(|s| point_segment(p, *s))
            .fold(f64::INFINITY, f64::min)
    }

    /// The points of `piece` of tag `tag`.
    fn tagged(piece: &Piece, tag: u32) -> Vec<[f64; 2]> {
        piece
            .points
            .iter()
            .zip(&piece.tags)
            .filter(|(_, t)| **t == tag)
            .map(|(p, _)| *p)
            .collect()
    }

    /// Hand check: a straight span along x from 0 to 300 m, its wire 5 m
    /// left of the axis (y = 5); 10 m away: the left line (tag 0) at
    /// y = 5 + 10 = 15, the right one (tag 1: the left of the span back)
    /// at y = 5 − 10 = −5 (the outermost conductor on the right is the
    /// same wire), every 2 m: 151 points each; round each end a half
    /// circle of 10 m about the wire's end (x < 0 and x > 300), so the
    /// line is closed, every point 10 m from the wire. With a second
    /// wire 5 m right of the axis, ±15 m. On flat ground at 400 m, the
    /// lines at 400 m.
    #[test]
    fn a_straight_span_gives_a_closed_line_at_the_distance_from_its_wire() {
        let wire = [[[0.0, 5.0], [300.0, 5.0]]];
        let g = span([0.0, 0.0], [300.0, 0.0], &wire);
        let pieces = all(&g, 10.0);
        assert_eq!(pieces.len(), 1);
        let line = &pieces[0];
        assert!(line.closed && line.points[0] == *line.points.last().unwrap());
        assert_simple(&line.points, "span");
        for (tag, y) in [(0, 15.0), (1, -5.0)] {
            let side = tagged(line, tag);
            assert_eq!(side.len(), 151, "tag {tag}");
            for p in &side {
                assert!((p[1] - y).abs() < 1e-9, "{tag}: {p:?}");
            }
        }
        let ends = tagged(line, NO_SIDE);
        assert!(ends.iter().any(|p| p[0] < -9.99) && ends.iter().any(|p| p[0] > 309.99));
        for p in &line.points {
            assert!((nearest(&wire, *p) - 10.0).abs() < 1e-9, "{p:?}");
        }
        let axis3 = [[0.0, 0.0, 400.0], [300.0, 0.0, 400.0]];
        let draped = drape(&line.points, Some(&flat(400.0)), &axis3);
        assert_eq!(draped.len(), 1);
        assert!(draped[0].iter().all(|p| (p[2] - 400.0).abs() < 1e-9));
        let wires = [[[0.0, 5.0], [300.0, 5.0]], [[0.0, -5.0], [300.0, -5.0]]];
        let line = &all(&span([0.0, 0.0], [300.0, 0.0], &wires), 10.0)[0];
        assert!(tagged(line, 0).iter().all(|p| (p[1] - 15.0).abs() < 1e-9));
        assert!(tagged(line, 1).iter().all(|p| (p[1] + 15.0).abs() < 1e-9));
    }

    /// From the axis: ±d, round the ends.
    #[test]
    fn from_the_axis_the_lines_are_at_the_distance() {
        let line = &all(&axis_graph(&[[0.0, 0.0], [100.0, 0.0]]), 7.5)[0];
        assert!(tagged(line, 0).iter().all(|p| (p[1] - 7.5).abs() < 1e-9));
        assert!(tagged(line, 1).iter().all(|p| (p[1] + 7.5).abs() < 1e-9));
        let axis = [[[0.0, 0.0], [100.0, 0.0]]];
        assert!(
            line.points
                .iter()
                .all(|p| (nearest(&axis, *p) - 7.5).abs() < 1e-9)
        );
    }

    /// A sharp angle (a 120° turn to the left): the line, closed round
    /// both ends, crosses itself nowhere; every point is `d` from the
    /// source (on the band's edge, none nearer); the outer corner round
    /// about the angle point, the inner one where the two sides meet.
    #[test]
    fn an_angle_gives_no_self_intersection() {
        let axis = [[0.0, 0.0], [200.0, 0.0], [100.0, 173.205]];
        let src: Vec<Seg> = axis.windows(2).map(|w| [w[0], w[1]]).collect();
        let d = 20.0;
        let pieces = all(&axis_graph(&axis), d);
        assert_eq!(pieces.len(), 1);
        let line = &pieces[0];
        assert!(line.closed);
        assert_simple(&line.points, "angle");
        for p in &line.points {
            let near = nearest(&src, *p);
            assert!((near - d).abs() < 1e-6, "{p:?} at {near}");
        }
        // The inner side (the left: tags 0 and 2): the loop cut off where
        // the two lines cross, d from both legs: on the bisector of the
        // 60° between them, d / sin 30° = 40 m from the angle point,
        // i.e. x = 200 − 20·√3 = 165.36, y = 20.
        let x = 200.0 - 20.0 * 3f64.sqrt();
        assert!(
            line.points
                .iter()
                .any(|p| (p[0] - x).abs() < 1e-3 && (p[1] - 20.0).abs() < 1e-6),
            "{:?}",
            line.points
        );
        // The outer side: 120° in steps of 5° about the angle point, 23
        // points between the two legs' ends, and those ends.
        let around = line
            .points
            .iter()
            .filter(|p| {
                ((p[0] - 200.0).hypot(p[1]) - d).abs() < 1e-6 && p[1] < 0.0 || {
                    ((p[0] - 200.0).hypot(p[1]) - d).abs() < 1e-6 && p[0] > 200.0
                }
            })
            .count();
        assert!(around >= 25, "{around}");
    }

    /// Blowout: the wire swung out 3.2 m mid-span moves the lines by as
    /// much there.
    #[test]
    fn the_lines_follow_the_blowout_of_the_outermost_conductor() {
        let still = [[[0.0, 5.0], [150.0, 5.0]], [[150.0, 5.0], [300.0, 5.0]]];
        // Swung to the left: 5 m at the clamps, 8.2 m in the middle.
        let blown = [[[0.0, 5.0], [150.0, 8.2]], [[150.0, 8.2], [300.0, 5.0]]];
        let at_mid = |src: &[Seg], tag| {
            let pieces = all(&span([0.0, 0.0], [300.0, 0.0], src), 10.0);
            tagged(&pieces[0], tag)
                .into_iter()
                .find(|q| (q[0] - 150.0).abs() < 1e-9)
                .unwrap()[1]
        };
        assert!((at_mid(&still, 0) - 15.0).abs() < 1e-9);
        assert!((at_mid(&still, 1) + 5.0).abs() < 1e-9);
        // 8.2 + 10.
        assert!((at_mid(&blown, 0) - 18.2).abs() < 1e-9);
        // The right side: the same wire, blown away from it: 8.2 − 10 =
        // −1.8; the bands of the two pieces, meeting at an angle of
        // 2 × 1.2°, reach 2.5 mm beyond it there.
        assert!((at_mid(&blown, 1) + 1.8).abs() < 0.005);
    }

    /// Two spans at an angle structure (a 30° turn to the right) with a
    /// wire 6 m each side of the axis: no self-intersection, every point
    /// 10 m from the nearest wire.
    #[test]
    fn an_angle_structure_with_wires() {
        let a = [0.0, 0.0];
        let b = [250.0, 0.0];
        let t = 30f64.to_radians();
        let c = [b[0] + 250.0 * t.cos(), -250.0 * t.sin()];
        // Wires along each span, ±6 m off its chord, meeting on the
        // bisector at the structure.
        let off = |p: [f64; 2], n: [f64; 2], o: f64| [p[0] + n[0] * o, p[1] + n[1] * o];
        let n1 = [0.0, 1.0];
        let n2 = [t.sin(), t.cos()];
        let bis = {
            let s = [n1[0] + n2[0], n1[1] + n2[1]];
            let l = s[0].hypot(s[1]);
            [s[0] / l, s[1] / l]
        };
        let k = 6.0 / 15f64.to_radians().cos();
        let mut spans = vec![Vec::new(), Vec::new()];
        for o in [-6.0f64, 6.0] {
            let at_b = off(b, bis, k * o.signum());
            spans[0].push([off(a, n1, o), at_b]);
            spans[1].push([at_b, off(c, n2, o)]);
        }
        let all_wires: Vec<Seg> = spans.iter().flatten().copied().collect();
        let g = Graph::path(vec![a, b, c], spans, Vec::new());
        let pieces = all(&g, 10.0);
        assert_eq!(pieces.len(), 1);
        assert_simple(&pieces[0].points, "angle structure");
        for p in &pieces[0].points {
            let near = nearest(&all_wires, *p);
            assert!((near - 10.0).abs() < 1e-6, "{p:?} at {near}");
        }
    }

    /// A span holding nothing: the parts on either side each closed
    /// round their ends.
    #[test]
    fn a_span_without_wires_parts_the_line() {
        let w = |y0: f64, x0: f64, x1: f64| [[x0, y0], [x1, y0]];
        let g = Graph::path(
            vec![[0.0, 0.0], [100.0, 0.0], [200.0, 0.0], [300.0, 0.0]],
            vec![
                vec![w(5.0, 0.0, 100.0)],
                Vec::new(),
                vec![w(5.0, 200.0, 300.0)],
            ],
            Vec::new(),
        );
        let pieces = all(&g, 10.0);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|p| p.closed));
    }

    /// A tap: three spans from one structure at (0, 0) — west, east and
    /// north, 100 m each, their chords the sources — 10 m away: one
    /// closed line round all three, no gap; the two inner corners cut
    /// where the sides meet, at (±10, 10); the south side straight
    /// through at y = −10; every point 10 m from the nearest span; round
    /// ends 10 m beyond each end.
    #[test]
    fn a_tap_gives_one_closed_line_without_gaps() {
        let o = [0.0, 0.0];
        let ends = [[-100.0, 0.0], [100.0, 0.0], [0.0, 100.0]];
        let mut g = Graph {
            nodes: vec![o, ends[0], ends[1], ends[2]],
            joints: vec![Vec::new(); 4],
            ..Graph::default()
        };
        for (k, e) in ends.iter().enumerate() {
            g.edges.push(Edge {
                a: 0,
                b: k + 1,
                sources: vec![[o, *e]],
                ..Edge::default()
            });
        }
        let src: Vec<Seg> = ends.iter().map(|e| [o, *e]).collect();
        let pieces = all(&g, 10.0);
        assert_eq!(pieces.len(), 1, "{pieces:?}");
        let line = &pieces[0];
        assert!(line.closed);
        assert_simple(&line.points, "tap");
        for p in &line.points {
            assert!((nearest(&src, *p) - 10.0).abs() < 1e-6, "{p:?}");
        }
        for x in [-10.0, 10.0] {
            assert!(
                line.points
                    .iter()
                    .any(|p| (p[0] - x).abs() < 1e-6 && (p[1] - 10.0).abs() < 1e-6),
                "inner corner at ({x}, 10)"
            );
        }
        assert!(
            line.points
                .iter()
                .any(|p| p[0].abs() < 1e-6 && (p[1] + 10.0).abs() < 1e-9)
        );
        for (e, beyond) in ends.iter().zip([[-110.0, 0.0], [110.0, 0.0], [0.0, 110.0]]) {
            let _ = e;
            assert!(
                line.points
                    .iter()
                    .any(|p| (p[0] - beyond[0]).hypot(p[1] - beyond[1]) < 0.5),
                "round the end at {beyond:?}"
            );
        }
    }

    /// The round end at a structure with a wide arm (a gantry): two wires
    /// ending 8 m either side of the span's axis, 3 m away: the line goes
    /// round the end at 3 m beyond the wires' ends (straight across
    /// between them, the outline of their hull), right up to the gantry,
    /// closed.
    #[test]
    fn the_line_closes_round_a_wide_end() {
        let wires = [[[0.0, 8.0], [200.0, 8.0]], [[0.0, -8.0], [200.0, -8.0]]];
        let g = span([0.0, 0.0], [200.0, 0.0], &wires);
        let line = &all(&g, 3.0)[0];
        assert!(line.closed);
        let ahead: Vec<[f64; 2]> = tagged(line, NO_SIDE)
            .into_iter()
            .filter(|p| p[0] > 200.0)
            .collect();
        // Straight across the end, 3 m beyond it, from one wire's end to
        // the other's.
        let across = line.points.windows(2).any(|w| {
            w.iter().all(|p| (p[0] - 203.0).abs() < 0.05) && w[0][1] > 7.9 && w[1][1] < -7.9
        });
        assert!(across, "{ahead:?}");
        for p in &line.points {
            assert!(nearest(&wires, *p) > 3.0 - 1e-9, "{p:?}");
        }
    }

    /// Two routes side by side, 12 m apart, not joined, 10 m away: one
    /// envelope over both — a single closed line, no point of it within
    /// 10 m of either route's wires (the lines between them gone) — or
    /// each its own: two closed lines, each crossing into the other's
    /// band. The routes as `groups` makes them: one group of every span
    /// drawn, or one per route; a route left out not drawn; a span on no
    /// route always drawn.
    #[test]
    fn the_envelope_over_two_routes_side_by_side() {
        let mut g = Graph {
            nodes: vec![
                [0.0, 0.0],
                [200.0, 0.0],
                [0.0, 12.0],
                [200.0, 12.0],
                [0.0, -50.0],
                [100.0, -50.0],
            ],
            joints: vec![Vec::new(); 6],
            ..Graph::default()
        };
        for (k, (a, b, route)) in [(0, 1, "r1"), (2, 3, "r2"), (4, 5, "")]
            .into_iter()
            .enumerate()
        {
            g.edges.push(Edge {
                a,
                b,
                sources: vec![[g.nodes[a], g.nodes[b]]],
                routes: if route.is_empty() {
                    Vec::new()
                } else {
                    vec![route.to_owned()]
                },
                along: Some((format!("a{k}"), true)),
            });
        }
        let both: Vec<Seg> = vec![g.edges[0].sources[0], g.edges[1].sources[0]];
        let envelope = outline(&g, &[0, 1], 10.0);
        assert_eq!(envelope.len(), 1, "{envelope:?}");
        assert!(envelope[0].closed);
        assert_simple(&envelope[0].points, "envelope");
        for p in &envelope[0].points {
            let near = nearest(&both, *p);
            assert!(near > 10.0 - 0.02 - 1e-6, "{p:?} at {near}");
        }
        // Outside: y = −10 and y = 22.
        assert!(
            envelope[0]
                .points
                .iter()
                .any(|p| (p[1] + 10.0).abs() < 1e-9)
        );
        assert!(
            envelope[0]
                .points
                .iter()
                .any(|p| (p[1] - 22.0).abs() < 1e-9)
        );
        assert!(
            !envelope[0]
                .points
                .iter()
                .any(|p| (p[1] - 2.0).abs() < 1e-6 && p[0] > 20.0 && p[0] < 180.0)
        );
        for e in [0, 1] {
            let own = outline(&g, &[e], 10.0);
            assert_eq!(own.len(), 1);
            assert!(own[0].closed);
            let other = [g.edges[1 - e].sources[0]];
            assert!(own[0].points.iter().any(|p| nearest(&other, *p) < 5.0));
        }
        // Grouped by the settings.
        let mut s = Settings::default();
        assert_eq!(groups(&g, &s), vec![(String::new(), vec![0, 1, 2])]);
        s.route_lines = RouteLines::Each;
        assert_eq!(
            groups(&g, &s),
            vec![
                ("r1".into(), vec![0]),
                ("r2".into(), vec![1]),
                (String::new(), vec![2])
            ]
        );
        s.hidden_routes.insert("r2".into());
        assert_eq!(
            groups(&g, &s),
            vec![("r1".into(), vec![0]), (String::new(), vec![2])]
        );
    }

    /// Draped on a TIN: every point's height is the surface's there; a
    /// point at each edge crossed (flat between them); broken where it
    /// leaves the surface.
    #[test]
    fn the_lines_take_the_heights_of_the_tin() {
        let mut pts = Vec::new();
        for i in -4..=16 {
            for j in -4..=4 {
                let (x, y) = (f64::from(i) * 25.0, f64::from(j) * 25.0);
                let bump = if (i + j) % 3 == 0 { 2.0 } else { 0.0 };
                pts.push([x, y, 100.0 + 0.1 * x + 0.3 * y.abs() + bump]);
            }
        }
        let g = Terrain::from_xyz(&pts, None).unwrap();
        let plan = [[0.0, 15.0], [123.0, 17.0], [250.0, 15.0]];
        let runs = drape(&plan, Some(&g), &[]);
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert!(run.len() > 20, "{}", run.len());
        for p in run {
            let z = g.height(p[0], p[1]).unwrap();
            assert!((p[2] - z).abs() < 1e-6, "{p:?} {z}");
        }
        for w in run.windows(2) {
            let m = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
            let z = g.height(m[0], m[1]).unwrap();
            assert!((z - (w[0][2] + w[1][2]) / 2.0).abs() < 1e-6);
        }
        let runs = drape(&[[0.0, 15.0], [600.0, 15.0]], Some(&g), &[]);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].last().unwrap()[0] <= 400.0 + 1e-6);
    }

    /// Without a ground surface: the alignment's heights, linear between
    /// its points, every 2 m.
    #[test]
    fn without_ground_the_alignments_heights() {
        let axis = [[0.0, 0.0, 400.0], [100.0, 0.0, 420.0]];
        let runs = drape(&[[0.0, 10.0], [100.0, 10.0]], None, &axis);
        assert_eq!(runs[0].len(), 51);
        for p in &runs[0] {
            assert!((p[2] - (400.0 + 0.2 * p[0])).abs() < 1e-9);
        }
    }

    /// The cases displayed (owner, 2026-10-22): the line's for every
    /// section and the jumpers, a section's own where it has them; each
    /// case once, in order.
    #[test]
    fn the_cases_displayed() {
        let mut d = Displayed::all(&[2, 0]);
        assert_eq!(d.cases(), vec![0, 2]);
        assert!(d.counts(Some("S1"), 2) && d.counts(None, 0));
        assert!(!d.counts(Some("S1"), 1));
        d.sections.insert("S2".into(), vec![1, 3]);
        assert_eq!(d.cases(), vec![0, 1, 2, 3]);
        assert!(d.counts(Some("S2"), 3) && !d.counts(Some("S2"), 2));
        assert!(d.counts(Some("S1"), 2) && !d.counts(None, 1));
    }
}
