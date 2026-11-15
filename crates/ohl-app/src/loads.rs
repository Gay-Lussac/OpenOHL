// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Structure loads: the loads the wires and strings put on the
//! structures in each load case (the load case file used: the project's,
//! Line › Structures › Load cases…), computed on demand in the background — the
//! model is not changed. One load case shown at a time; factored or not;
//! a click goes to the structure; CSV export. **Usage**: how much of its
//! capacity each structure uses (its type's allowable spans and loads,
//! structure model and pole, [`ohl_loads::capacity`]), the governing load
//! case or one case, worst first. A structure whose loads in a case lack
//! a section or jumper that did not solve there is **incomplete**: its
//! rows are greyed, "not checked: S12 not solved in ice", never a pass.
//! **Colour structures by usage** colours them in the plan and 3D views
//! ([`UsageMark`], with a legend): incomplete ones in a neutral colour,
//! "not checked".

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use ohl_loads::CaseLoads;
use ohl_loads::capacity::{CaseUsage, StructureUsage, UsageStatus};
// Usage from `ohl_structures_fe::check::NEAR_FULL` is shown as a warning
// (orange): one classification for both applications.
use ohl_structures_fe::check::{UsageClass, classify_usage};

use crate::check_help as help;
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// Its **Use local conditions**: the wind taken, the check's name for
/// the run times, what the wind is for.
const LOCAL: (ohl_line::local::Purpose, &str, &str) = (
    ohl_line::local::Purpose::Strength,
    "structure-loads",
    "every load case as if it said use_local = true: the wires solved with their spans' wind \
     and ice for strength, and the wind on the structures' bodies (wind bands, the members of \
     the usage check, the poles) times each structure's factor, once; a case's own \
     [case.structure] ice = true takes each structure's ice. Kept with the project: the loads \
     sent to OHL-Structure (Open in OHL-Structure…) follow it, so both checks take the same \
     cases",
);

/// What a run computes from (taken on the main thread).
pub struct Inputs {
    /// The line (not localized: the load cases localize it themselves).
    pub line: ohl_line::Line,
    /// Its wires.
    pub library: ohl_cables::Library,
    /// The structure types.
    pub types: ohl_structures::StructureLibrary,
    /// The weather cases.
    pub weather: Vec<ohl_weather::WeatherCase>,
    /// The cross-section library (or why it does not read).
    pub sections: Result<ohl_sections::SectionLibrary, String>,
    /// The load cases of the library, or why there are none.
    pub cases: Result<Vec<ohl_loads::LoadCase>, String>,
    /// The names of every case of the load case library (the groups are
    /// checked against them, not against the cases run).
    pub known: Vec<String>,
    /// **Use local conditions** ticked.
    pub local: bool,
    /// Per structure, the load cases it is computed for (the window's
    /// setup and the line's groups, [`crate::loads_plan`]); none: every
    /// structure in every case it takes.
    pub plan: Option<crate::loads_plan::Plan>,
}

/// A load case of the run that was not computed, with why (said in the
/// window, never silently left out).
#[derive(Debug, Clone, PartialEq)]
pub struct NotComputed {
    /// The load case (its name in the library).
    pub case: String,
    /// Why ("load case X: no weather case \"Y\"").
    pub why: String,
    /// The structures that were to be computed in it (not checked in it).
    pub structures: Vec<String>,
}

/// Where a run is (2026-11-15, owner: "a progressbar indicating where we
/// are at. first solve the lines then the structures"): read by the
/// window while the run's threads write it.
#[derive(Debug, Default)]
pub struct Stages {
    /// 0: starting, 1: solving the line in each load case, 2: the
    /// structures, 3: done.
    pub stage: std::sync::atomic::AtomicUsize,
    /// Done in the stage.
    pub done: std::sync::atomic::AtomicUsize,
    /// Of.
    pub total: std::sync::atomic::AtomicUsize,
    /// When the stage began, ms after the run's start.
    pub since_ms: std::sync::atomic::AtomicU64,
    /// When the structures began (the line solved), ms after the start.
    pub structures_ms: std::sync::atomic::AtomicU64,
    /// When the run ended, ms after the start.
    pub ended_ms: std::sync::atomic::AtomicU64,
    /// Stops the run (Stop): no answer.
    pub cancel: ohl_core::budget::CancelToken,
}

impl Stages {
    /// Stage `stage` begins with `total` to do, `ms` after the start.
    fn begin(&self, stage: usize, total: usize, ms: u64) {
        use std::sync::atomic::Ordering::Relaxed;
        self.done.store(0, Relaxed);
        self.total.store(total, Relaxed);
        self.since_ms.store(ms, Relaxed);
        self.stage.store(stage, Relaxed);
    }

    /// The footer's bar and words, `elapsed` after the start.
    pub fn progress(&self, elapsed: std::time::Duration) -> crate::check_ui::Progress {
        use std::sync::atomic::Ordering::Relaxed;
        let (stage, done, total) = (
            self.stage.load(Relaxed),
            self.done.load(Relaxed),
            self.total.load(Relaxed),
        );
        let since = std::time::Duration::from_millis(self.since_ms.load(Relaxed));
        let what = match stage {
            1 => format!(
                "Solving the line: case {} of {total}",
                (done + 1).min(total)
            ),
            2 => format!("Structures: {done} of {total}"),
            _ => "Starting".to_owned(),
        };
        // The time left in this stage, from its own pace.
        let in_stage = elapsed.saturating_sub(since);
        let p = crate::check_ui::Progress::of(in_stage, done, total);
        crate::check_ui::Progress {
            fraction: p.fraction,
            text: format!(
                "{what} — {:.0} s{}",
                elapsed.as_secs_f64(),
                match p.text.split_once(", about ") {
                    Some((_, left)) => format!(", about {left} in this stage"),
                    None => String::new(),
                }
            ),
        }
    }
}

/// The answer of a run.
#[derive(Debug, Clone)]
pub struct Run {
    /// The loads per load case computed (or why none).
    pub loads: Outcome,
    /// The usage of the structures, per load case computed.
    pub usage: Vec<CaseUsage>,
    /// The load cases not computed, with why.
    pub failed: Vec<NotComputed>,
    /// Per (structure, load case): who gives it the case (its groups, no
    /// group, the case's own list), in words.
    pub given: std::collections::BTreeMap<(String, String), String>,
}

/// The loads and their usage (one rule for the local conditions, owner
/// 2026-11-03): the window's switch sets `use_local` on every case
/// ([`ohl_loads::with_local`]) and the line goes in as it is, so
/// `ohl_loads` solves the localized wires and takes each structure's
/// `wind_static` once on its body wind (bands, members, poles); the
/// structures' ice and snow rows come with
/// [`ohl_loads::add_structure_loads`].
#[cfg(test)]
pub fn compute(i: &Inputs) -> (Outcome, Vec<CaseUsage>) {
    let r = run(i, &Stages::default(), 0).expect("not cancelled");
    (r.loads, r.usage)
}

/// The loads and their usage in two stages, `stages` telling where it is (none:
/// cancelled): **1** the line solved in each load case needed (the cases
/// in parallel, `threads` at once; 0: as many as the cores) — a case that
/// does not compute fails only itself ([`Run::failed`]); **2** the
/// structures checked in parallel, each in every case it is computed for
/// (its member checks once per case, on the solved forces). The answer is
/// the same with one thread or many (each case and each structure is
/// computed on its own).
pub fn run(i: &Inputs, stages: &Stages, threads: usize) -> Option<Run> {
    use std::sync::atomic::Ordering::Relaxed;
    let started = Instant::now();
    let ms = || started.elapsed().as_millis() as u64;
    let line = &i.line;
    let library_cases = match i.cases.clone() {
        Ok(c) => c,
        Err(e) => {
            return Some(Run::refused(e));
        }
    };
    let sections = match i.sections.clone() {
        Ok(s) => s,
        Err(e) => {
            return Some(Run::refused(e));
        }
    };
    // The cases some structure is computed for, as the library has them.
    let cases: Vec<ohl_loads::LoadCase> = library_cases
        .into_iter()
        .filter(|c| i.plan.as_ref().is_none_or(|p| p.solve.contains(&c.name)))
        .collect();
    let cases = ohl_loads::with_local(cases, i.local);
    // A case of several wind directions: one case per direction.
    let expanded = ohl_loads::expand_directions(&cases);
    let threads = match threads {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
        n => n,
    };
    // Stage 1: the line in each case (the cases in parallel; each solve
    // as one after the other would).
    stages.begin(1, expanded.len(), ms());
    let budget = ohl_loads::default_budget().cancel_token(stages.cancel.clone());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let solve_one = || {
        let mut mine = Vec::new();
        loop {
            if stages.cancel.is_cancelled() {
                break;
            }
            let k = next.fetch_add(1, Relaxed);
            let Some(case) = expanded.get(k) else { break };
            let r = ohl_loads::structure_loads_each(
                line,
                &i.library,
                &i.weather,
                std::slice::from_ref(case),
                &i.known,
                &budget,
            )
            .into_iter()
            .next()
            .unwrap_or_else(|| Err(format!("load case {}: not computed", case.name)));
            mine.push((k, r));
            stages.done.fetch_add(1, Relaxed);
        }
        mine
    };
    let mut each: Vec<Option<Result<CaseLoads, String>>> = vec![None; expanded.len()];
    let workers = threads.clamp(1, expanded.len().max(1));
    if workers == 1 {
        for (k, r) in solve_one() {
            each[k] = Some(r);
        }
    } else {
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..workers).map(|_| s.spawn(solve_one)).collect();
            for h in handles {
                for (k, r) in h.join().expect("a line solve panicked") {
                    each[k] = Some(r);
                }
            }
        });
    }
    if stages.cancel.is_cancelled() {
        return None;
    }
    let sets = line.case_sets();
    let mut loads = Vec::new();
    let mut failed: Vec<NotComputed> = Vec::new();
    for (case, r) in expanded.iter().zip(each) {
        match r? {
            Ok(l) => loads.push(l),
            Err(why) => {
                let name = case.set_name().to_owned();
                let structures = match &i.plan {
                    Some(p) => p
                        .order
                        .iter()
                        .filter(|s| p.computes(s, &name))
                        .cloned()
                        .collect(),
                    None => sets.takers(&name),
                };
                failed.push(NotComputed {
                    case: name,
                    why,
                    structures,
                });
            }
        }
    }
    ohl_loads::add_structure_loads(&mut loads, line, &i.types, &sections, &i.weather);
    // Each structure in the cases it is computed for only.
    if let Some(p) = &i.plan {
        for c in loads.iter_mut() {
            let name = c.case.set_name().to_owned();
            c.loads.retain(|l| p.computes(&l.structure, &name));
            c.missing.retain(|m| p.computes(&m.structure, &name));
            for b in &mut c.broken_each {
                b.loads.retain(|l| p.computes(&l.structure, &name));
            }
        }
    }
    let mut given = std::collections::BTreeMap::new();
    for c in &loads {
        let name = c.case.set_name();
        for l in &c.loads {
            given
                .entry((l.structure.clone(), name.to_owned()))
                .or_insert_with(|| crate::loads_plan::given_by(&sets, &l.structure, name));
        }
    }
    // Stage 2: the structures, in parallel.
    let structures: std::collections::BTreeSet<&str> = loads
        .iter()
        .flat_map(|c| c.loads.iter().map(|l| l.structure.as_str()))
        .collect();
    stages.begin(2, structures.len(), ms());
    stages.structures_ms.store(ms(), Relaxed);
    let report = |n: usize, of: usize| {
        stages.done.store(n, Relaxed);
        stages.total.store(of, Relaxed);
    };
    let usage = ohl_loads::capacity::usage_with_progress(
        line,
        &i.types,
        &sections,
        &i.library,
        &i.weather,
        &loads,
        &ohl_loads::capacity::Progress {
            threads,
            done: Some(&report),
            cancel: Some(&stages.cancel),
        },
    )?;
    stages.begin(3, 0, ms());
    stages.ended_ms.store(ms(), Relaxed);
    Some(Run {
        loads: Ok(loads),
        usage,
        failed,
        given,
    })
}

type Outcome = Result<Vec<CaseLoads>, String>;

impl Run {
    /// No run: why.
    fn refused(why: String) -> Self {
        Self {
            loads: Err(why),
            usage: Vec::new(),
            failed: Vec::new(),
            given: Default::default(),
        }
    }
}

/// A computation's answer: the run (none: stopped) and the notes of the
/// local conditions.
type Finished = (Option<Run>, Vec<String>);

/// Colour of what is not checked because a wire did not solve (grey).
const INCOMPLETE: egui::Color32 = egui::Color32::from_rgb(150, 156, 170);

/// How a structure is coloured by usage in the plan and 3D views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageMark {
    /// Below 90 %.
    Within,
    /// From 90 % to 100 %.
    Near,
    /// Over 100 % (or not allowed).
    Over,
    /// Not checked: a section or jumper at it did not solve (its loads
    /// are incomplete).
    Incomplete,
    /// Approximate: its finite-element check kept tension-only members
    /// active because their set did not settle.
    Approximate,
    /// Within its capacity, but part of a member's check not done (not on
    /// the safe side, 2026-11-08): never shown as a pass.
    NotSafe,
    /// Nothing to check against (no type, no capacity).
    Unchecked,
}

impl UsageMark {
    /// Every mark, in the legend's order.
    pub const ALL: [UsageMark; 7] = [
        UsageMark::Within,
        UsageMark::Near,
        UsageMark::Over,
        UsageMark::NotSafe,
        UsageMark::Incomplete,
        UsageMark::Approximate,
        UsageMark::Unchecked,
    ];

    /// The mark of a structure's usage (in one case, or its governing one).
    pub fn of(s: &StructureUsage) -> Self {
        match s.status() {
            UsageStatus::Incomplete => Self::Incomplete,
            UsageStatus::Approximate => Self::Approximate,
            UsageStatus::NotSafe => Self::NotSafe,
            UsageStatus::NotChecked => Self::Unchecked,
            UsageStatus::Over => Self::Over,
            UsageStatus::Pass => match s.governing() {
                Some(g) if classify_usage(g.usage) == UsageClass::Near => Self::Near,
                _ => Self::Within,
            },
        }
    }

    /// Its colour.
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Within => [110, 200, 120],
            Self::Near => [255, 170, 60],
            Self::Over => [235, 70, 60],
            Self::Incomplete => [150, 156, 170],
            Self::Approximate => [175, 120, 220],
            Self::NotSafe => [215, 110, 200],
            Self::Unchecked => [95, 100, 110],
        }
    }

    /// In the legend.
    pub fn label(self) -> &'static str {
        match self {
            Self::Within => "below 90 %",
            Self::Near => "90 – 100 %",
            Self::Over => "over 100 %",
            Self::Incomplete => "not checked: a wire not solved",
            Self::Approximate => "approximate (tension-only kept)",
            Self::NotSafe => "not all checked (not safe side)",
            Self::Unchecked => "not checked (no capacity, a case)",
        }
    }
}

/// The legend of the colours by usage, at the bottom right of `rect`.
pub fn paint_usage_legend(painter: &egui::Painter, rect: egui::Rect, out_of_date: bool) {
    let font = egui::FontId::proportional(11.0);
    let row = 15.0;
    let width = 190.0;
    let height = row * UsageMark::ALL.len() as f32 + 22.0;
    let at = rect.right_bottom() + egui::vec2(-8.0 - width, -8.0 - height);
    let frame = egui::Rect::from_min_size(at, egui::vec2(width, height));
    painter.rect_filled(frame, 4.0, egui::Color32::from_black_alpha(170));
    painter.text(
        at + egui::vec2(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        if out_of_date {
            "structures by usage: OUT OF DATE"
        } else {
            "structures by usage"
        },
        font.clone(),
        if out_of_date {
            egui::Color32::from_rgb(255, 190, 60)
        } else {
            egui::Color32::WHITE
        },
    );
    for (k, m) in UsageMark::ALL.iter().enumerate() {
        let y = at.y + 20.0 + row * k as f32;
        let [r, g, b] = m.color();
        painter.circle_filled(
            egui::pos2(at.x + 12.0, y + 6.0),
            4.5,
            egui::Color32::from_rgb(r, g, b),
        );
        painter.text(
            egui::pos2(at.x + 22.0, y),
            egui::Align2::LEFT_TOP,
            m.label(),
            font.clone(),
            egui::Color32::WHITE,
        );
    }
}

/// The loads of case `r` at the structures whose id contains `filter`
/// (lower case; empty: all), in the case's order.
fn loads_shown<'a>(
    r: &'a CaseLoads,
    filter: &'a str,
) -> impl Iterator<Item = &'a ohl_loads::AttachmentLoad> {
    r.loads
        .iter()
        .filter(move |l| filter.is_empty() || l.structure.to_lowercase().contains(filter))
}

/// A row of the loads table: load `l` of case `r`, factored or not;
/// incomplete (a wire's forces missing at its structure) as a violation.
fn load_row(
    r: &CaseLoads,
    l: &ohl_loads::AttachmentLoad,
    unfactored: bool,
    given: &str,
) -> crate::check_ui::Row {
    use crate::check_ui::Cell;
    let missing: Vec<String> = r.missing_at(&l.structure).map(|m| m.short()).collect();
    let v = if unfactored { l.local } else { l.factored };
    let sections = if missing.is_empty() {
        l.sections.join(", ")
    } else {
        format!(
            "{} — incomplete: {}",
            l.sections.join(", "),
            missing.join(", ")
        )
    };
    crate::check_ui::Row::new(
        vec![
            Cell::text(&l.structure).bad(!missing.is_empty()),
            Cell::text(&l.label),
            Cell::text(
                l.sets
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            Cell::num(Some(v[0] / 1000.0), 2),
            Cell::num(Some(v[1] / 1000.0), 2),
            Cell::num(Some(v[2] / 1000.0), 2),
            Cell::text(sections).bad(!missing.is_empty()),
            Cell::text(given),
        ],
        !missing.is_empty(),
    )
}

/// The window's own page of the user guide (its numbered sections).
pub const GUIDE: &str = "user-guide/structure-loads-window";

/// The sections of [`GUIDE`] the window's **user guide ↗** buttons open
/// (each a heading of the page: tested).
pub mod anchors {
    /// *1 What it checks*.
    pub const WHAT: &str = "1-what-it-checks";
    /// *2 Load cases*.
    pub const CASES: &str = "2-load-cases";
    /// *3 Structure groups*.
    pub const GROUPS: &str = "3-structure-groups";
    /// *4 Structures in no group*.
    pub const ALONE: &str = "4-structures-in-no-group";
    /// *5 Before Run*.
    pub const BEFORE: &str = "5-before-run";
    /// *Results*.
    pub const RESULTS: &str = "results";
}

/// The page and section a **user guide ↗** button opens.
pub fn guide(anchor: &'static str) -> (&'static str, &'static str) {
    (GUIDE, anchor)
}

/// A load case of the library as the window lists it.
#[derive(Debug, Clone, PartialEq)]
struct CaseInfo {
    /// Its name.
    name: String,
    /// Its weather case.
    weather: String,
    /// The project has no weather case of that name: not computable.
    weather_missing: bool,
}

impl CaseInfo {
    /// Its state before Run, in words (none: computable).
    fn problem(&self) -> Option<String> {
        self.weather_missing.then(|| {
            format!(
                "not computable: no weather case \"{}\" in the project",
                self.weather
            )
        })
    }
}

/// What the library was read from: the file, the libraries' revision
/// ([`crate::revision::Revisions::libraries`], counted up when a library
/// page saves) and the file's time — read again when any changes (owner
/// 2026-11-15: "when the load cases in the library are edited and saved,
/// the compute load case doesn't update").
type LibraryKey = (std::path::PathBuf, u64, Option<std::time::SystemTime>);

/// What the plan was made from: the line's version, the libraries'
/// revision, the setup, the route worked on and the case names.
type PlanKey = (
    u64,
    u64,
    crate::loads_plan::Choice,
    Option<std::collections::BTreeSet<String>>,
    Vec<String>,
);

/// The plan and what the setup lists with it.
struct Planned {
    /// Per structure, the cases computed.
    plan: crate::loads_plan::Plan,
    /// The structures on an alignment.
    aligned: std::collections::BTreeSet<String>,
    /// The line's groups resolved.
    sets: ohl_line::CaseSets,
}

/// The Structure loads window (a page until 2026-10-27; now docked as a
/// panel or floating like every check window, [`crate::check_help::window`]).
/// It computes only when Run is pressed (owner 2026-11-15: "the compute
/// structure loads starts immediately when the window is opened. i would
/// like to have to click on run to start it").
#[derive(Default)]
pub struct LoadsPage {
    /// Shown.
    pub open: bool,
    /// The last results (or why there are none).
    pub results: Option<Outcome>,
    /// **Use local conditions** (owner, 2026-11-03): the loads with the
    /// structures' wind for strength and wire ice on their spans, and their
    /// wind for strength on their own wind bands.
    pub local: bool,
    /// A computation running: since, its answer, where it is.
    running: Option<(Instant, Receiver<Finished>, std::sync::Arc<Stages>)>,
    /// How much of their capacity the structures use, per load case (with
    /// the last results).
    pub usage: Vec<CaseUsage>,
    /// The usage shown instead of the loads.
    pub show_usage: bool,
    /// How long the last one took.
    pub took: Option<std::time::Duration>,
    /// The last run's notes (Use local conditions, the weather groups no
    /// longer read).
    pub notes: Vec<String>,
    /// The load case shown.
    pub case: usize,
    /// Structures shown (id contains this text).
    pub filter: String,
    /// Factored loads shown (else unfactored).
    pub unfactored: bool,
    /// The usage's load case: 0 the governing one, k + 1 case k.
    pub usage_case: usize,
    /// The structures coloured by usage in the plan and 3D views (the
    /// usage's load case).
    pub colour_structures: bool,
    /// What the results were computed from ([`crate::revision`]).
    stamp: crate::revision::Stamp,
    /// The setup: the groups run, the cases of the structures in no group,
    /// the structures ticked (kept in the project, [`SETTING`]).
    pub choice: crate::loads_plan::Choice,
    /// The load case library as last read (with what from), or why not.
    library: Option<(LibraryKey, Result<Vec<CaseInfo>, String>)>,
    /// The plan as last made, with what from.
    planned: Option<(PlanKey, std::sync::Arc<Planned>)>,
    /// The plan of the results shown.
    run_plan: Option<crate::loads_plan::Plan>,
    /// The load cases of the results shown that were not computed.
    pub failed: Vec<NotComputed>,
    /// Who gave each structure each case in the results shown.
    given: std::collections::BTreeMap<(String, String), String>,
    /// Counts the answers taken (a new one opens the Results tab).
    generation: u64,
    /// The loads table's rows, by the answers taken, the load case
    /// shown, factored or not and the structure filter.
    rows: crate::check_ui::CachedRows<(u64, usize, bool, String)>,
    /// How much the last run computed (structure × case checks) and the
    /// run under way: its time estimates the next.
    units: (usize, usize),
    /// Its setup kept in the project (2026-11-14: the placed structures
    /// sent to OHL-Structure follow Use local conditions, so it must
    /// outlive the window; [`SETTING`]).
    kept: crate::check_ui::Kept,
    /// **Make these a group…** asked: the structures to select before the
    /// Load cases per group window opens.
    pub make_group: Option<Vec<String>>,
    /// A section of the user guide asked.
    pub guide: Option<&'static str>,
    /// Runs started (a test: opening the window starts none).
    pub runs_started: u64,
}

/// The project setting keeping the Structure loads window's setup, per
/// project line: `{"local": true, "groups_off": […], "alone_cases": […],
/// "structures_off": […], "structures_on": […]}`
/// ([`crate::loads_plan::Choice`]).
pub const SETTING: &str = "app.structure_loads";

impl LoadsPage {
    /// Reads the setup from the project when not read yet for the line
    /// worked on, and writes it into it when it changed ([`SETTING`];
    /// called every frame, the window open or not: what OHL-Structure gets
    /// follows Use local conditions).
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        self.choice.local = self.local;
        let json = serde_json::to_string(&self.choice).unwrap_or_default();
        let mut kept = std::mem::take(&mut self.kept);
        if let crate::check_ui::Step::Load(text) =
            kept.step(model, ctx, SETTING, "structure loads settings", &json)
        {
            self.choice = text
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default();
            self.local = self.choice.local;
            kept.loaded(serde_json::to_string(&self.choice).unwrap_or_default());
        }
        self.kept = kept;
    }

    /// Whether a computation runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The load cases of the results shown that were not computed, with
    /// why.
    #[cfg(test)]
    pub fn not_computed(&self) -> &[NotComputed] {
        &self.failed
    }

    /// The colour of each structure by usage in the usage's load case (the
    /// governing one: incomplete in any case is incomplete; a structure
    /// that was to be computed in a case not computed is not checked, never
    /// a pass), when colouring by usage is on (else none).
    pub fn usage_marks(&self) -> std::collections::BTreeMap<String, UsageMark> {
        let mut out = std::collections::BTreeMap::new();
        if !self.colour_structures {
            return out;
        }
        match self.usage_case {
            0 => {
                for c in &self.usage {
                    for s in &c.structures {
                        out.entry(s.structure.clone())
                            .or_insert(UsageMark::Unchecked);
                    }
                }
                for (_, s) in ohl_loads::capacity::governing(&self.usage) {
                    out.insert(s.structure.clone(), UsageMark::of(s));
                }
                for f in &self.failed {
                    for s in &f.structures {
                        let m = out.entry(s.clone()).or_insert(UsageMark::Unchecked);
                        if matches!(m, UsageMark::Within | UsageMark::Near) {
                            *m = UsageMark::Unchecked;
                        }
                    }
                }
                for c in &self.usage {
                    for s in c.structures.iter().filter(|s| s.is_incomplete()) {
                        out.insert(s.structure.clone(), UsageMark::Incomplete);
                    }
                }
            }
            k => {
                if let Some(c) = self.usage.get(k - 1) {
                    for s in &c.structures {
                        out.insert(s.structure.clone(), UsageMark::of(s));
                    }
                }
            }
        }
        out
    }

    /// The load cases not computed that structure `id` was to be computed
    /// in.
    fn failed_at(&self, id: &str) -> Vec<&NotComputed> {
        self.failed
            .iter()
            .filter(|f| f.structures.iter().any(|s| s == id))
            .collect()
    }

    /// The load case library's cases (read again when the library
    /// changed: saved in its page, another file, the file's time), or
    /// why not; none without a file.
    fn library(&mut self, model: &Model) -> Option<&Result<Vec<CaseInfo>, String>> {
        let path = model.sources.load_cases.clone()?;
        let time = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let key: LibraryKey = (path.clone(), model.revisions.libraries, time);
        if self.library.as_ref().is_none_or(|(k, _)| *k != key) {
            let cases = ohl_loads::LoadCases::load(&path).map(|set| {
                set.cases
                    .into_iter()
                    .map(|c| CaseInfo {
                        weather_missing: !model.cases.iter().any(|w| w.case.name == c.weather),
                        name: c.name,
                        weather: c.weather,
                    })
                    .collect()
            });
            self.library = Some((key, cases));
        }
        self.library.as_ref().map(|(_, c)| c)
    }

    /// The library's case names (none: no file, or it does not read).
    fn names(&mut self, model: &Model) -> Vec<String> {
        match self.library(model) {
            Some(Ok(c)) => c.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// The plan of the setup on the model now (made again when the line,
    /// a library, the setup or the route worked on changed).
    fn planned(&mut self, model: &Model) -> std::sync::Arc<Planned> {
        let names = self.names(model);
        let scope = model.scope().map(|s| s.structures);
        let key: PlanKey = (
            model.line_version,
            model.revisions.libraries,
            self.choice.clone(),
            scope.clone(),
            names.clone(),
        );
        if let Some((k, p)) = &self.planned
            && *k == key
        {
            return p.clone();
        }
        let sets = model.line.case_sets();
        let aligned = crate::loads_plan::on_alignment(&model.line);
        let plan = crate::loads_plan::plan(
            &model.line,
            &sets,
            &names,
            &self.choice,
            scope.as_ref(),
            &aligned,
        );
        let p = std::sync::Arc::new(Planned {
            plan,
            aligned,
            sets,
        });
        self.planned = Some((key, p.clone()));
        p
    }

    /// Computes the loads of the model as it is now (in the background),
    /// for the plan of the setup.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = model.sources.load_cases.clone() else {
            self.results = Some(Err(
                "no load case file: Line › Structures › Load cases… (the project's own, or a \
                 .ohllc file next to a line file)"
                    .into(),
            ));
            return;
        };
        // Its notes and run times (the line itself goes in as it is: the
        // load cases take the local conditions, one rule).
        let local = crate::local_solve::prepare(&model.line, self.local, LOCAL.0);
        let planned = self.planned(model);
        let plan = planned.plan.clone();
        let library = ohl_loads::LoadCases::load(&path);
        let known: Vec<String> = library
            .as_ref()
            .map(|set| set.cases.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        let cases = library.and_then(|set| {
            if plan.solve.is_empty() {
                return Err(
                    "nothing to compute: tick a group, a structure or a load case \
                            (Setup)"
                        .to_owned(),
                );
            }
            Ok(set.cases)
        });
        let inputs = Inputs {
            line: model.line.clone(),
            library: model.library.clone(),
            types: model
                .types
                .as_ref()
                .map(|(t, _)| t.clone())
                .unwrap_or_default(),
            weather: model.cases.iter().map(|c| c.case.clone()).collect(),
            // The structure models' section library (the project's).
            sections: model
                .section_library()
                .map_err(|e| format!("cross-section library: {e}")),
            cases,
            known,
            local: self.local,
            plan: Some(plan.clone()),
        };
        self.units.1 = plan.pairs();
        let (tx, rx) = channel();
        let c = ctx.clone();
        self.stamp.started(model.revision());
        let stages = std::sync::Arc::new(Stages::default());
        let progress = stages.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let r = run(&inputs, &progress, 0);
            local.finish(LOCAL.1, started.elapsed());
            let _ = tx.send((r, local.notes()));
            c.request_repaint();
        });
        self.running = Some((Instant::now(), rx, stages));
        self.run_plan = Some(plan);
        self.runs_started += 1;
    }

    /// Stops the run under way (its threads stop between cases and
    /// structures); the previous results stay.
    fn stop(&mut self) {
        if let Some((_, _, stages)) = self.running.take() {
            stages.cancel.cancel();
        }
    }

    /// Takes a finished computation.
    pub fn poll(&mut self) {
        let Some((t, rx, _)) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok((Some(run), notes)) => {
                self.usage = run.usage;
                self.failed = run.failed;
                self.given = run.given;
                self.notes = notes;
                Some(run.loads)
            }
            // Stopped: the previous results stay.
            Ok((None, _)) => {
                self.running = None;
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.usage.clear();
                self.failed.clear();
                Some(Err("the computation stopped".into()))
            }
        };
        if let Some(r) = r {
            self.took = Some(t.elapsed());
            if let Ok(v) = &r
                && self.case >= v.len()
            {
                self.case = 0;
            }
            self.results = Some(r);
            self.stamp.arrived();
            self.running = None;
            self.generation += 1;
            self.units.0 = self.units.1;
        }
    }

    /// What the results were computed from that changed since, in words
    /// (`None`: nothing, or no results): the line, its cases and files
    /// (the load case library among them, [`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.results, Some(Ok(_)))
            .then(|| self.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The window: Setup and Results tabs (owner, 2026-10-31, the check
    /// pages' guidelines), Run in the footer at the bottom, always
    /// visible; returns a structure to go to. Nothing is computed before
    /// Run.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut go = None;
        let planned = self.planned(model);
        let plan = &planned.plan;
        let library_problem = match self.library(model) {
            Some(Err(e)) => Some(format!("the load case file does not read: {e}")),
            _ => None,
        };
        let why = if model.sources.load_cases.is_none() {
            Some(
                "no load case file: add one in the Load cases library (Line › Structures › Load \
                 cases…)"
                    .to_owned(),
            )
        } else if library_problem.is_some() {
            library_problem
        } else if plan.order.is_empty() {
            Some("nothing to compute: tick a group, a structure or a load case (Setup)".to_owned())
        } else {
            None
        };
        let findings = match &self.results {
            Some(Ok(r)) => Some(r.iter().map(|c| c.loads.len()).sum()),
            _ => None,
        };
        let tab = crate::check_ui::tabs(
            ui,
            "loads-page",
            findings,
            self.generation,
            why.is_some() && !self.running(),
        );
        match tab {
            crate::check_ui::Tab::Setup => {
                help::body(ui, "loads-page", |ui| self.setup_ui(ui, model));
            }
            crate::check_ui::Tab::Results => {
                go = crate::check_ui::results_area(ui, "loads-page", |ui| self.body_ui(ui, model));
            }
        }
        let units = plan.pairs();
        let estimate = match self.took {
            Some(t) if self.units.0 > 0 && matches!(self.results, Some(Ok(_))) => format!(
                " — {} (from the last run)",
                crate::check_ui::plan_duration(
                    t.as_secs_f64() * units as f64 / self.units.0 as f64
                )
            ),
            _ => " — time measured on the first run".to_owned(),
        };
        let last = self
            .took
            .map(|t| format!(" · last run {:.1} s", t.as_secs_f64()))
            .unwrap_or_default();
        help::footer(ui, "loads-page", |ui| {
            // The shared Run footer (2026-10-27): Run greyed with its
            // reason, Stop while it runs (the previous results stay); the
            // bar says the stage and where it is (2026-11-15).
            let f = crate::check_ui::RunFooter {
                running: self
                    .running
                    .as_ref()
                    .map(|(t, _, stages)| stages.progress(t.elapsed())),
                why_not: why,
                hover: "Solve the line in every load case needed, then check each structure ticked \
                        in the cases it takes (the structures in parallel)",
                scope: None,
                info: Some(format!(
                    "{} · {}{estimate}{last}",
                    crate::check_ui::count(plan.order.len(), "structure"),
                    crate::check_ui::count(plan.solve.len(), "load case"),
                )),
            };
            let local = &mut self.local;
            match crate::check_ui::run_footer(ui, f, |ui| {
                crate::local_solve::switch(ui, local, LOCAL.1, LOCAL.0, LOCAL.2, &model.line);
            }) {
                crate::check_ui::Action::Run => self.start(model, ui.ctx()),
                crate::check_ui::Action::Stop => self.stop(),
                crate::check_ui::Action::None => {}
            }
        });
        go
    }

    /// The Setup tab: 1 What it checks (with its drawing), 2 Load cases
    /// (the library's, each with its state), 3 Structure groups (each run
    /// or not, its structures and cases; the cases' own lists), 4
    /// Structures in no group (the cases run for them, a choice of this
    /// window), 5 Before Run (what a run computes).
    fn setup_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        use crate::check_ui;
        let mut guide = None;
        guide = guide.or(check_ui::section(
            ui,
            1,
            "What it checks",
            Some(anchors::WHAT),
        ));
        help::explain(
            ui,
            "The loads the wires and strings put on each structure, at every point they hang \
             from, in each load case it takes; and how much of its capacity each structure uses.",
            "Every section is solved in the load case's weather case (finite elements); the \
             forces at its clamps are carried up the strings to the structure points, \
             transverse, longitudinal and vertical in the structure's axes, times the load \
             case's factors; the wind on the structure is added. The usage compares them with \
             the type's allowable spans and loads, its structure model and its pole. A run \
             first solves the line in every load case needed (the cases in parallel), then \
             checks the structures (in parallel), the member checks once per structure and \
             case on the solved forces. The model is not changed.",
        );
        check_ui::how_computed(
            ui,
            "loads-page",
            "Per load case: the weather case's wires solved; at each structure point the sum of \
             its strings' forces, split into T (across the line, the bisector at an angle), L \
             (along it) and V (down); factored by the case's V, T and L factors (unfactored \
             shown on request).",
            help::structure_loads,
        );
        ui.separator();
        guide = guide.or(check_ui::section_help(
            ui,
            2,
            "Load cases",
            Some(anchors::CASES),
            Some((
                "The load case library's cases, each with its state.",
                "The cases of the project's load case library (Line › Structures › Load cases…), \
                 read again whenever the library is saved. A case whose weather case the \
                 project lacks cannot be computed: it is said here and in the results, never \
                 left out silently (add the weather case in Line › Criteria › Weather cases, or \
                 with the library's Add the default cases…). Which structures take which case \
                 is set by the structure groups (3) and, for the structures in no group, here \
                 (4).",
            )),
        ));
        let Some(path) = model.sources.load_cases.clone() else {
            // Expected in a new project: not an error.
            ui.label("No load case file yet: the structure loads are computed for its cases.");
            ui.label(
                egui::RichText::new(
                    "Add one in the Load cases library (Line › Structures › Load cases…): \
                     Browse… for a .ohllc file, New file… from a template, or Add the default \
                     cases….",
                )
                .small()
                .weak(),
            );
            if ui.button("Open the Load cases library").clicked() {
                crate::menu::request_page(
                    ui.ctx(),
                    crate::app::Tab::Library(crate::library::LibraryKind::LoadCases),
                );
            }
            self.guide = guide;
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Load case file");
            ui.label(
                egui::RichText::new(
                    path.file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                )
                .strong(),
            );
            if ui.button("Load cases library…").clicked() {
                crate::menu::request_page(
                    ui.ctx(),
                    crate::app::Tab::Library(crate::library::LibraryKind::LoadCases),
                );
            }
        });
        let planned = self.planned(model);
        let (plan, sets, aligned) = (&planned.plan, &planned.sets, &planned.aligned);
        let cases: Vec<CaseInfo> = match self.library(model) {
            Some(Ok(c)) => c.clone(),
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                self.guide = guide;
                return;
            }
            None => Vec::new(),
        };
        let running = self.running();
        ui.label(
            egui::RichText::new(format!(
                "{} in the library; {} computed by the setup below",
                check_ui::count(cases.len(), "load case"),
                plan.solve.len()
            ))
            .small(),
        );
        help::list(
            ui,
            "loads-library-cases",
            150.0,
            egui::ScrollArea::vertical()
                .id_salt("loads-library-cases")
                .auto_shrink([false, true]),
            |ui| {
                egui::Grid::new("loads-library-cases-grid")
                    .striped(true)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        for h in ["load case", "weather case", "taken by", "state"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for c in &cases {
                            ui.label(&c.name);
                            ui.label(&c.weather);
                            let takers = sets.takers(&c.name).len();
                            let computed =
                                plan.cases.values().filter(|v| v.contains(&c.name)).count();
                            ui.label(format!("{takers} structure(s); {computed} computed"));
                            match c.problem() {
                                Some(p) => {
                                    ui.colored_label(ui.visuals().error_fg_color, p);
                                }
                                None if takers == 0 => {
                                    ui.label(
                                        egui::RichText::new(
                                            "taken by no structure (the groups, its own list)",
                                        )
                                        .weak(),
                                    );
                                }
                                None if computed == 0 => {
                                    ui.label(
                                        egui::RichText::new("not computed (the setup below)")
                                            .weak(),
                                    );
                                }
                                None => {
                                    ui.label("computed on Run");
                                }
                            }
                            ui.end_row();
                        }
                    });
            },
        );
        ui.separator();
        // 3 Structure groups.
        let groups: Vec<(usize, &ohl_line::StructureGroup)> = model
            .line
            .structure_groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.is_own_list())
            .collect();
        guide = guide.or(check_ui::section_help(
            ui,
            3,
            "Structure groups",
            Some(anchors::GROUPS),
            Some((
                "Each group run or not: its structures computed in the cases it gives.",
                "The groups of Line › Structures › Load cases per group: a structure takes the \
                 load cases of every group it is in (both groups' when in two); a load case's \
                 own list adds its case to the structures it picks (or whose ground its rule \
                 holds for) and takes it from those it excludes. Untick a group: its \
                 structures are not computed in this run; a structure also in a group run is \
                 computed for the cases of the groups run only (said in 5). The line's groups \
                 are not changed here. The structures on no alignment are at the bottom of \
                 each list, left out unless ticked.",
            )),
        ));
        if groups.is_empty() {
            ui.label(
                egui::RichText::new(
                    "The line has no structure groups: every structure takes every case (4). \
                     Groups are made in Line › Structures › Load cases per group….",
                )
                .small()
                .weak(),
            );
        }
        let mut choice = self.choice.clone();
        ui.add_enabled_ui(!running, |ui| {
            for (k, g) in &groups {
                let members = sets.members(*k);
                let mut on = choice.runs_group(&g.name);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .checkbox(&mut on, egui::RichText::new(&g.name).strong())
                        .on_hover_text("Run this group: its structures computed in its cases")
                        .changed()
                    {
                        choice.groups_off.retain(|x| x != &g.name);
                        if !on {
                            choice.groups_off.push(g.name.clone());
                        }
                    }
                    let computed = members
                        .iter()
                        .filter(|s| plan.cases.contains_key(*s))
                        .count();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} ({computed} computed) — gives {}",
                            check_ui::count(members.len(), "structure"),
                            if g.load_cases.is_empty() {
                                "no load case (its structures take none from it)".to_owned()
                            } else {
                                g.load_cases.join(", ")
                            }
                        ))
                        .small(),
                    );
                });
                egui::CollapsingHeader::new(format!("{}: its structures", g.name))
                    .id_salt(("loads-group", k))
                    .show(ui, |ui| {
                        structure_ticks(
                            ui,
                            ("loads-group-ticks", *k),
                            members,
                            aligned,
                            &mut choice,
                            model,
                        );
                    });
            }
            // The cases' own lists: who takes a case beyond the groups.
            let own: Vec<(usize, &ohl_line::StructureGroup)> = model
                .line
                .structure_groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.is_own_list())
                .collect();
            if !own.is_empty() {
                ui.label(egui::RichText::new("Load cases' own lists").strong())
                    .on_hover_text(
                        "Set in Load cases… › Applies to: a case's own list adds the case to the \
                         structures it picks (or whose ground its rule holds for) on top of their \
                         groups' cases, and takes it from those it excludes. Not run or unrun \
                         here: they decide who takes their case.",
                    );
                for (k, g) in own {
                    let case = g.load_case.as_deref().unwrap_or_default();
                    let mut parts = Vec::new();
                    if !g.structures.is_empty() {
                        parts.push(format!("picks {}", g.structures.join(", ")));
                    }
                    if !g.rules.is_empty() {
                        parts.push(format!("rule: {}", g.rules.words(str::to_owned)));
                    }
                    if !sets.excluded(k).is_empty() {
                        parts.push(format!("excludes {}", sets.excluded(k).join(", ")));
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} — {} — taken by {} structure(s)",
                            g.label(),
                            if parts.is_empty() {
                                "no pick, no rule".to_owned()
                            } else {
                                parts.join("; ")
                            },
                            sets.takers(case).len()
                        ))
                        .small(),
                    );
                }
            }
        });
        ui.separator();
        // 4 Structures in no group.
        guide = guide.or(check_ui::section_help(
            ui,
            4,
            "Structures in no group",
            Some(anchors::ALONE),
            Some((
                "Every case of the library by the groups' rule: tick the cases run for them.",
                "A structure in no group takes every case of the library (less the cases whose \
                 own list excludes it). The cases ticked here are those computed for them in \
                 this run: a choice of this window, kept with its setup in the project; the \
                 line's groups are not changed (to give them cases for good, make them a group: \
                 Make these a group…). The structures on no alignment are at the bottom, left \
                 out unless ticked.",
            )),
        ));
        let alone: Vec<String> = sets.in_no_group().to_vec();
        if alone.is_empty() {
            ui.label(
                egui::RichText::new("Every structure is in a group.")
                    .small()
                    .weak(),
            );
        } else {
            ui.label(
                egui::RichText::new(format!(
                    "{} in no group: they take every case of the library{}",
                    check_ui::count(alone.len(), "structure"),
                    {
                        let out: Vec<String> = alone
                            .iter()
                            .filter_map(|id| match sets.of(id) {
                                ohl_line::CaseSet::EveryBut(c) => {
                                    Some(format!("{id} not {}", c.join(", ")))
                                }
                                _ => None,
                            })
                            .collect();
                        if out.is_empty() {
                            String::new()
                        } else {
                            format!(" (their own lists exclude: {})", out.join("; "))
                        }
                    }
                ))
                .small(),
            );
            ui.add_enabled_ui(!running, |ui| {
                ui.columns(2, |cols| {
                    cols[0].strong("Load cases run for them");
                    let items: Vec<crate::tick_list::Item> = cases
                        .iter()
                        .map(|c| {
                            crate::tick_list::Item::new(
                                &c.name,
                                match c.problem() {
                                    Some(p) => format!("{} — {p}", c.name),
                                    None => c.name.clone(),
                                },
                            )
                        })
                        .collect();
                    crate::tick_list::tick_list(
                        &mut cols[0],
                        "loads-alone-cases",
                        &items,
                        &mut choice.alone_cases,
                        160.0,
                    );
                    cols[1].strong("Structures");
                    structure_ticks(
                        &mut cols[1],
                        "loads-alone-structures",
                        &alone,
                        aligned,
                        &mut choice,
                        model,
                    );
                });
                if ui
                    .button("Make these a group…")
                    .on_hover_text(
                        "Selects the structures in no group that are ticked and opens Load cases \
                         per group, where New group and Add the selected make them a group that \
                         ticks its own cases (optional)",
                    )
                    .clicked()
                {
                    self.make_group = Some(
                        alone
                            .iter()
                            .filter(|id| choice.ticks(id, aligned.contains(*id)))
                            .cloned()
                            .collect(),
                    );
                }
            });
        }
        if choice != self.choice {
            self.choice = choice;
        }
        ui.separator();
        // 5 Before Run.
        guide = guide.or(check_ui::section_help(
            ui,
            5,
            "Before Run",
            Some(anchors::BEFORE),
            Some((
                "What Run computes, and what it leaves out (said, never silent).",
                "Run solves the line in each load case listed (the cases in parallel), then \
                 checks each structure computed in the cases it takes (the structures in \
                 parallel). Nothing is computed before Run; the results of an earlier run stay \
                 shown, marked out of date when the line or a library changed since.",
            )),
        ));
        let planned = self.planned(model);
        let plan = &planned.plan;
        let unknown: Vec<String> = cases
            .iter()
            .filter(|c| plan.solve.contains(&c.name) && c.weather_missing)
            .map(|c| c.name.clone())
            .collect();
        ui.label(format!(
            "{} computed · {} solved · {} structure checks",
            check_ui::count(plan.order.len(), "structure"),
            check_ui::count(plan.solve.len(), "load case"),
            plan.pairs()
        ));
        let mut lines: Vec<(bool, String)> = Vec::new();
        if !unknown.is_empty() {
            lines.push((
                true,
                format!(
                    "not computable (no weather case of their name in the project): {} — the \
                     structures taking them are not checked in them",
                    unknown.join(", ")
                ),
            ));
        }
        for (id, off) in &plan.partly {
            lines.push((
                false,
                format!(
                    "{id}: in a group run and in {} not run — computed for the cases of the \
                     groups run only",
                    off.join(", ")
                ),
            ));
        }
        let mut why_count: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
        for (id, why) in &plan.skipped {
            why_count.entry(why.as_str()).or_default().push(id);
        }
        for (why, ids) in why_count {
            lines.push((
                false,
                format!("not computed ({why}): {} — {}", ids.len(), ids.join(", ")),
            ));
        }
        if !plan.none.is_empty() {
            lines.push((
                true,
                format!(
                    "{}: {} — no loads, not checked",
                    ohl_loads::TAKES_NO_LOAD_CASE,
                    plan.none.join(", ")
                ),
            ));
        }
        if !lines.is_empty() {
            help::list(
                ui,
                "loads-before-run",
                120.0,
                egui::ScrollArea::vertical()
                    .id_salt("loads-before-run")
                    .auto_shrink([false, true]),
                |ui| {
                    for (bad, l) in lines {
                        if bad {
                            ui.colored_label(ui.visuals().warn_fg_color, l);
                        } else {
                            ui.label(egui::RichText::new(l).small());
                        }
                    }
                },
            );
        }
        self.guide = guide;
    }

    /// The load cases of the results not computed (with why and the
    /// structures not checked in them) and the structures the run left
    /// out (with why), in a box of its own height.
    fn not_computed_ui(&self, ui: &mut egui::Ui) {
        let skipped = self.run_plan.as_ref().map_or(&[][..], |p| &p.skipped[..]);
        if self.failed.is_empty() && skipped.is_empty() {
            return;
        }
        help::list(
            ui,
            "loads-not-computed",
            110.0,
            egui::ScrollArea::vertical()
                .id_salt("loads-not-computed")
                .auto_shrink([false, true]),
            |ui| {
                for f in &self.failed {
                    crate::menu::error_label(
                        ui,
                        format!(
                            "not computed: {} — {}{}",
                            f.case,
                            f.why,
                            if f.structures.is_empty() {
                                String::new()
                            } else {
                                format!(" — not checked in it: {}", f.structures.join(", "))
                            }
                        ),
                    );
                }
                let mut by: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
                for (id, why) in skipped {
                    by.entry(why.as_str()).or_default().push(id);
                }
                for (why, ids) in by {
                    ui.colored_label(
                        INCOMPLETE,
                        format!("not computed ({why}): {}", ids.join(", ")),
                    );
                }
            },
        );
    }

    /// The Results tab.
    pub(crate) fn body_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut go = None;
        if model.sources.load_cases.is_none() {
            ui.label("No load case file yet (Setup › 2 Load cases).");
            return None;
        }
        let results = match &self.results {
            None => {
                ui.label(
                    "The loads the wires and strings put on the structures, at every \
                     attachment, in each load case. Run to see them.",
                );
                return None;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return None;
            }
            Some(Ok(r)) => r,
        };
        if let Some(a) = help::heading(ui, "Results", anchors::RESULTS) {
            self.guide = Some(a);
        }
        if let Some(what) = self.out_of_date(model)
            && crate::revision::banner(ui, &what, !self.running())
        {
            self.start(model, ui.ctx());
            return None;
        }
        // What the run did not compute, with why (never silently missing).
        self.not_computed_ui(ui);
        if results.is_empty() {
            if self.failed.is_empty() {
                ui.label("No load case was computed (Setup › 5 Before Run).");
            }
            return None;
        }
        for n in &self.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.show_usage, false, "Loads");
            ui.selectable_value(&mut self.show_usage, true, "Usage")
                .on_hover_text(
                    "How much of its capacity each structure uses: its type's allowable spans \
                     (Method 1), allowable loads (Method 2), structure model (fe) and pole",
                );
        });
        if self.show_usage {
            let names: Vec<String> = results.iter().map(|r| r.case.name.clone()).collect();
            // A case with sections not solved misses their loads: said so.
            let incomplete: Vec<String> = results
                .iter()
                .filter(|r| !r.failed.is_empty())
                .map(|r| {
                    let mut at: Vec<&str> =
                        r.missing.iter().map(|m| m.structure.as_str()).collect();
                    at.dedup();
                    format!(
                        "{} ({} sections or jumpers; incomplete at {} structures)",
                        r.case.name,
                        r.failed.len(),
                        at.len()
                    )
                })
                .collect();
            if !incomplete.is_empty() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "not solved in: {} — the structures they hang on are not checked there \
                         (grey); see Loads",
                        incomplete.join(", ")
                    ),
                );
            }
            return self.usage_ui(ui, &names);
        }
        ui.horizontal(|ui| {
            let names: Vec<&str> = results.iter().map(|r| r.case.name.as_str()).collect();
            // Labels before their controls (the desktop app's rule).
            ui.label("Load case");
            egui::ComboBox::from_id_salt("structure-loads-case")
                .selected_text(names[self.case.min(names.len() - 1)])
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.case, k, *n);
                    }
                });
            ui.label("Structure");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(100.0));
            ui.checkbox(&mut self.unfactored, "unfactored");
            // The table's Export CSV… writes the case shown; this one
            // every case (named apart, check pages' guidelines §4).
            if ui
                .button("Export every case (CSV)…")
                .on_hover_text(
                    "Every load case, every attachment, as one CSV file (the table's Export \
                     CSV… writes the case shown)",
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("structure-loads.csv")
                    .save_file()
                && let Err(e) = std::fs::write(&path, ohl_loads::to_csv(results))
            {
                log::warn!("{}: {e}", path.display());
            }
        });
        let r = &results[self.case.min(results.len() - 1)];
        let c = &r.case;
        ui.label(
            egui::RichText::new(format!(
                "{} — weather {}, wires {}, factors V {} T {} L {}, strength {}{}",
                if c.description.is_empty() {
                    &c.name
                } else {
                    &c.description
                },
                c.weather,
                c.condition.as_str(),
                c.vertical,
                c.transverse,
                c.longitudinal,
                c.strength,
                if c.tags.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", c.tags.join(", "))
                }
            ))
            .small(),
        );
        // The structure groups: only the structures whose groups give
        // this case (2026-11-15).
        if model.line.has_load_case_sets() {
            let sets = model.line.case_sets();
            let (with, without): (Vec<&ohl_line::Structure>, Vec<&ohl_line::Structure>) = model
                .line
                .structures
                .iter()
                .partition(|s| sets.takes(&s.id, &c.name));
            let none = sets.without_load_cases();
            ui.label(
                egui::RichText::new(format!(
                    "taken by {} of {} structures (their groups); not by: {}{}",
                    with.len(),
                    model.line.structures.len(),
                    if without.is_empty() {
                        "—".to_owned()
                    } else {
                        without
                            .iter()
                            .map(|s| s.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                    if none.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " — of them {}: {} (not checked)",
                            ohl_loads::TAKES_NO_LOAD_CASE,
                            none.join(", ")
                        )
                    }
                ))
                .small()
                .weak(),
            )
            .on_hover_text(
                "Each structure takes the load cases of its structure groups (Line › Structures \
                 › Load cases per group; both groups' when it is in two; in no group, every \
                 case). A structure not taking this case has no loads in it; one whose groups \
                 tick no case has no loads in any and is not checked — listed as such, never as \
                 passing.",
            );
        }
        // The settings of Loads for structures of before 2026-11-15: said
        // once, kept as read and no longer used.
        if let Some(note) = model.line.unused_structure_loads_note() {
            ui.label(egui::RichText::new(note).small().weak());
        }
        // What the case assumed (wind side, local conditions, removed and
        // broken wires, ice and snow on the structures), in a box of its
        // own height.
        if !r.notes.is_empty() {
            help::list(
                ui,
                "loads-case-notes",
                90.0,
                egui::ScrollArea::vertical()
                    .id_salt("loads-case-notes")
                    .auto_shrink([false, true]),
                |ui| {
                    for n in &r.notes {
                        ui.label(egui::RichText::new(format!("• {n}")).small());
                    }
                },
            );
        }
        if !r.failed.is_empty() {
            // In a box of its own height, scrolled: a long list stays small.
            help::list(
                ui,
                "loads-list-1",
                130.0,
                egui::ScrollArea::vertical()
                    .id_salt("loads-not-solved")
                    .auto_shrink([false, true]),
                |ui| {
                    for f in &r.failed {
                        crate::menu::error_label(ui, format!("not solved: {f}"));
                    }
                    // The structures whose loads lack them.
                    let mut by: std::collections::BTreeMap<&str, Vec<String>> =
                        std::collections::BTreeMap::new();
                    for m in &r.missing {
                        by.entry(m.structure.as_str()).or_default().push(m.short());
                    }
                    for (s, wires) in by {
                        ui.colored_label(
                            INCOMPLETE,
                            format!(
                                "{s}: incomplete — not checked: {} in {}",
                                wires.join(", "),
                                r.case.name
                            ),
                        );
                    }
                },
            );
        }
        // The shared results table (owner, 2026-10-28): sortable, units in
        // the headers, searched, grouped by structure, CSV; a row clicked
        // goes to its structure; incomplete loads in the warning colour.
        // Built once per answer, load case shown, structure filter and
        // factored or not; not every frame.
        let filter = self.filter.trim().to_lowercase();
        use crate::check_ui::Col;
        let (case, unfactored) = (self.case.min(results.len() - 1), self.unfactored);
        let key = (self.generation, case, unfactored, filter.clone());
        let given = &self.given;
        let rows = self.rows.get_or_build(key, || {
            loads_shown(r, &filter)
                .map(|l| {
                    let by = given
                        .get(&(l.structure.clone(), r.case.set_name().to_owned()))
                        .map_or("", String::as_str);
                    load_row(r, l, unfactored, by)
                })
                .collect()
        });
        if let Some(k) = crate::check_ui::ResultTable::new(
            "structure-loads-rows",
            vec![
                Col::text("Structure"),
                Col::text("Point"),
                Col::text("Sets"),
                Col::num("Transverse [kN]"),
                Col::num("Longitudinal [kN]"),
                Col::num("Vertical [kN]"),
                Col::text("Sections"),
                Col::text("Given by"),
            ],
        )
        .rows_kept(rows)
        .no_filter()
        .group_by(&[0])
        .csv("structure-loads-table.csv")
        .ui(ui)
        {
            go = loads_shown(r, &filter).nth(k).map(|l| l.structure.clone());
        }
        go
    }

    /// The usage view; returns a structure to go to.
    fn usage_ui(&mut self, ui: &mut egui::Ui, names: &[String]) -> Option<String> {
        let mut go = None;
        // Load case: 0 = the governing one of each structure, k + 1 = case k.
        ui.horizontal_wrapped(|ui| {
            let shown = match self.usage_case {
                0 => "governing (every case)".to_owned(),
                k => names.get(k - 1).cloned().unwrap_or_default(),
            };
            ui.label("Load case");
            egui::ComboBox::from_id_salt("structure-usage-case")
                .selected_text(shown)
                .width(220.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.usage_case, 0, "governing (every case)");
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.usage_case, k + 1, n);
                    }
                });
            ui.label("Structure");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(100.0));
            if ui
                .button("Export CSV…")
                .on_hover_text("Every load case, every structure")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("structure-usage.csv")
                    .save_file()
                && let Err(e) = std::fs::write(&path, ohl_loads::capacity::usage_csv(&self.usage))
            {
                log::warn!("{}: {e}", path.display());
            }
            ui.checkbox(&mut self.colour_structures, "Colour structures by usage")
                .on_hover_text(
                    "The structures coloured in the plan and 3D views by their usage in the \
                     load case chosen here (a legend in the views); those not checked because a \
                     wire at them did not solve in a neutral colour",
                );
        });
        let rows: Vec<(&str, &StructureUsage)> = match self.usage_case {
            0 => ohl_loads::capacity::governing(&self.usage),
            k => self.usage.get(k - 1).map_or_else(Vec::new, |c| {
                let mut v: Vec<(&str, &StructureUsage)> =
                    c.structures.iter().map(|s| (c.case.as_str(), s)).collect();
                v.sort_by(|a, b| {
                    let u = |s: &StructureUsage| s.governing().map_or(-1.0, |g| g.usage);
                    u(b.1).total_cmp(&u(a.1))
                });
                v
            }),
        };
        // Incomplete: in the case shown, or (governing) in any case — with
        // the cases and the wires not solved.
        let not_checked = |s: &StructureUsage| -> Option<String> {
            match self.usage_case {
                0 => {
                    let cases = ohl_loads::capacity::incomplete_cases(&self.usage, &s.structure);
                    (!cases.is_empty()).then(|| {
                        let mut wires: Vec<String> = Vec::new();
                        for (_, u) in &cases {
                            for m in &u.missing {
                                let w = m.short();
                                if !wires.contains(&w) {
                                    wires.push(w);
                                }
                            }
                        }
                        format!(
                            "not checked: {} in {}",
                            wires.join(", "),
                            cases.iter().map(|c| c.0).collect::<Vec<_>>().join(", ")
                        )
                    })
                }
                k if s.is_incomplete() => Some(format!(
                    "{} in {}",
                    s.not_checked_text(),
                    names.get(k - 1).map_or("", String::as_str)
                )),
                _ => None,
            }
            // A case of its set not in the library (2026-11-14): not
            // checked in it, never a pass.
            .or_else(|| (!s.unknown_cases.is_empty()).then(|| s.not_checked_text()))
            // A case it was to be computed in that was not computed
            // (2026-11-15): the governing usage misses it, never a pass.
            .or_else(|| {
                let failed = self.failed_at(&s.structure);
                (self.usage_case == 0 && !failed.is_empty()).then(|| {
                    format!(
                        "not checked in {}: not computed",
                        failed
                            .iter()
                            .map(|f| f.case.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
            })
        };
        let checked = rows
            .iter()
            .filter(|r| r.1.governing().is_some() && not_checked(r.1).is_none())
            .count();
        let over = rows
            .iter()
            .filter(|r| {
                r.1.governing()
                    .is_some_and(|g| classify_usage(g.usage) == UsageClass::Over)
            })
            .count();
        let incomplete: Vec<&StructureUsage> = match self.usage_case {
            0 => {
                let mut seen = std::collections::BTreeSet::new();
                self.usage
                    .iter()
                    .flat_map(|c| c.structures.iter())
                    .filter(|s| s.is_incomplete() && seen.insert(s.structure.clone()))
                    .collect()
            }
            k => self.usage.get(k - 1).map_or_else(Vec::new, |c| {
                c.structures.iter().filter(|s| s.is_incomplete()).collect()
            }),
        };
        ui.label(
            egui::RichText::new(format!(
                "{checked} structures checked, {over} over 100 %, {} not checked (a wire not \
                 solved){}{}",
                incomplete.len(),
                {
                    // A case of its set not in the library (2026-11-14).
                    let mut seen = std::collections::BTreeSet::new();
                    for c in &self.usage {
                        for s in c.structures.iter().filter(|s| !s.unknown_cases.is_empty()) {
                            seen.insert(s.structure.as_str());
                        }
                    }
                    if seen.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ", {} not checked (a load case of its groups not in the library: {})",
                            seen.len(),
                            seen.into_iter().collect::<Vec<_>>().join(", ")
                        )
                    }
                },
                if checked == 0 {
                    " — give the structure types a [capacity] (allowable spans, \
                     allowable loads) to check them"
                } else {
                    ""
                }
            ))
            .small(),
        );
        // Not checked: at the end, with why.
        let unchecked: Vec<&StructureUsage> = match self.usage_case {
            0 => self
                .usage
                .first()
                .map(|c| {
                    c.structures
                        .iter()
                        .filter(|s| s.governing().is_none())
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let filter = self.filter.trim().to_lowercase();
        let (error, ok) = (ui.visuals().error_fg_color, crate::menu::ok_color(ui));
        let pct = |c: Option<&ohl_loads::capacity::Check>| match c {
            None => egui::RichText::new("—").weak(),
            Some(c) => {
                let text = if c.usage.is_finite() {
                    format!("{:.0} %", c.usage * 100.0)
                } else {
                    "not allowed".into()
                };
                let colour = match classify_usage(c.usage) {
                    UsageClass::Over => error,
                    _ if c.not_safe > 0 => {
                        let [r, g, b] = UsageMark::NotSafe.color();
                        egui::Color32::from_rgb(r, g, b)
                    }
                    UsageClass::Near => egui::Color32::from_rgb(255, 170, 60),
                    UsageClass::Pass | UsageClass::NotSafe => ok,
                };
                egui::RichText::new(text).monospace().color(colour)
            }
        };
        // Both ways: its many columns scroll sideways in the window's
        // 860 px rather than widening it.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("structure-usage")
                    .striped(true)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        for h in [
                            "structure",
                            "type",
                            "usage",
                            "spans",
                            "loads",
                            "fe",
                            "pole",
                            "governs",
                            "load case",
                            "angle °",
                            "wind span m",
                            "weight span m",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        let listed: std::collections::BTreeSet<&str> = rows
                            .iter()
                            .filter(|r| r.1.governing().is_some())
                            .map(|r| r.1.structure.as_str())
                            .chain(unchecked.iter().map(|s| s.structure.as_str()))
                            .collect();
                        // Incomplete without any check: listed too.
                        let extra: Vec<(&str, &StructureUsage)> = incomplete
                            .iter()
                            .filter(|s| !listed.contains(s.structure.as_str()))
                            .map(|s| ("", *s))
                            .collect();
                        let shown = rows
                            .iter()
                            .filter(|r| r.1.governing().is_some())
                            .chain(
                                unchecked
                                    .iter()
                                    .map(|s| ("", *s))
                                    .collect::<Vec<_>>()
                                    .iter(),
                            )
                            .chain(extra.iter())
                            .filter(|r| {
                                filter.is_empty() || r.1.structure.to_lowercase().contains(&filter)
                            })
                            .take(5000)
                            .cloned()
                            .collect::<Vec<_>>();
                        for (case, s) in shown {
                            let not = not_checked(s);
                            // Incomplete: grey, "not checked", the usage
                            // of the loads there are only on hover.
                            let grey = |t: egui::RichText| {
                                if not.is_some() {
                                    t.color(INCOMPLETE).italics()
                                } else {
                                    t
                                }
                            };
                            if ui
                                .link(grey(egui::RichText::new(&s.structure)))
                                .on_hover_text("Go to this structure")
                                .clicked()
                            {
                                go = Some(s.structure.clone());
                            }
                            ui.label(grey(egui::RichText::new(
                                s.structure_type.as_deref().unwrap_or("—"),
                            )));
                            let g = s.governing();
                            if not.is_some() {
                                let r = ui.label(
                                    egui::RichText::new("not checked")
                                        .color(INCOMPLETE)
                                        .italics(),
                                );
                                if let Some(g) = g {
                                    r.on_hover_text(format!(
                                        "{:.0} % from the loads there are (a wire is missing)",
                                        g.usage * 100.0
                                    ));
                                }
                                for c in [&s.spans, &s.loads, &s.fe, &s.pole] {
                                    ui.label(
                                        egui::RichText::new(if c.is_some() { "?" } else { "—" })
                                            .color(INCOMPLETE),
                                    );
                                }
                            } else {
                                ui.label(pct(g));
                                ui.label(pct(s.spans.as_ref()));
                                ui.label(pct(s.loads.as_ref()));
                                ui.label(pct(s.fe.as_ref()));
                                ui.label(pct(s.pole.as_ref()));
                            }
                            match (&not, g) {
                                (Some(text), _) => ui.colored_label(INCOMPLETE, text),
                                (None, Some(g)) => ui.label(&g.governs),
                                (None, None) => {
                                    ui.label(egui::RichText::new(s.notes.join("; ")).weak())
                                }
                            };
                            ui.label(case);
                            ui.label(format!("{:.1}", s.line_angle_deg));
                            ui.label(format!("{:.0}", s.wind_span));
                            ui.label(s.weight_span.map_or(String::new(), |[lo, hi]| {
                                if (hi - lo).abs() < 0.5 {
                                    format!("{hi:.0}")
                                } else {
                                    format!("{lo:.0} – {hi:.0}")
                                }
                            }));
                            ui.end_row();
                        }
                    });
            });
        go
    }
}

/// The tick lists of structures `ids`: those on an alignment, then those
/// on none under their own heading, left out unless ticked (owner
/// 2026-11-15); the ticks go into `choice` (one state per structure, the
/// same in every list it is in).
fn structure_ticks(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    ids: &[String],
    aligned: &std::collections::BTreeSet<String>,
    choice: &mut crate::loads_plan::Choice,
    model: &Model,
) {
    let label = |s: &str| match model
        .line
        .structure(s)
        .and_then(|x| x.structure_type.as_deref())
    {
        Some(t) => format!("{s} — {t}"),
        None => s.to_owned(),
    };
    let (on, off): (Vec<&String>, Vec<&String>) = ids.iter().partition(|s| aligned.contains(*s));
    for (part, list, on_alignment) in [(0, on, true), (1, off, false)] {
        if list.is_empty() {
            continue;
        }
        if !on_alignment {
            ui.label(
                egui::RichText::new("Not on an alignment (not computed unless ticked)")
                    .small()
                    .strong(),
            );
        }
        let items: Vec<crate::tick_list::Item> = list
            .iter()
            .map(|s| crate::tick_list::Item::new(s.as_str(), label(s)))
            .collect();
        let mut chosen = Some(
            list.iter()
                .filter(|s| choice.ticks(s, on_alignment))
                .map(|s| (*s).clone())
                .collect::<Vec<_>>(),
        );
        if crate::tick_list::tick_list(ui, (id, part), &items, &mut chosen, 140.0) {
            for s in &list {
                choice.tick(s, on_alignment, crate::tick_list::ticked(&chosen, s));
            }
        }
    }
}
