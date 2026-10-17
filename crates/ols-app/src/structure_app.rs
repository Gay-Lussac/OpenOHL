// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! **Edit** on a structure: the structure application (`ols-structure`)
//! started on the structure's file (in a project, its working copy), and
//! the file watched: when it is saved there, the line application stores
//! it into the project and reads the libraries again (as a save of the
//! Structure types page does). The file's time is looked at twice a
//! second at most, only while a structure application runs or a change is
//! waiting.
//!
//! On a **placed structure** (Properties › Edit…) the application gets
//! that place's loads: the line's values there in every load case
//! (`ols_structure_link::placed_values`) written beside the structure
//! file as `<stem>.<structure>.line-values.json`
//! (`ols_structure_loads::line_values_path`) and passed with
//! `--line-values FILE --structure ID`; written again (in the background)
//! when the line changes while the application runs, and read again
//! there. The values typed over its load table come back as
//! `<stem>.<structure>.loads.csv` and are kept in the project (library
//! kind `loads`, beside the structure types) when they change. The
//! project's section library goes along as `--sections`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime};

use ols_loads::CaseLoads;
use ols_structure_loads::PlacedValues;

use crate::model::Model;

/// The structure application's program name.
pub const PROGRAM: &str = "ols-structure";

/// How often the files are looked at.
const POLL: Duration = Duration::from_millis(500);

/// How long the line stays unchanged before its values are computed
/// again (a drag changes it many times).
const SETTLE: Duration = Duration::from_millis(1000);

/// A placed structure open in the structure application: whose loads,
/// in which line, and its files beside the structure file.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// The placed structure's id.
    pub structure: String,
    /// Its line (the project line, or the line file's name).
    pub line: String,
    /// Its structure type.
    pub structure_type: String,
    /// The line-values file the application reads.
    pub values: PathBuf,
    /// The values typed over its load table (written by the application).
    pub overrides: PathBuf,
}

impl Placed {
    /// Placed structure `structure` (type `structure_type`) of `line`,
    /// edited in structure file `file`.
    pub fn new(file: &Path, structure: &str, line: &str, structure_type: &str) -> Self {
        Self {
            structure: structure.to_owned(),
            line: line.to_owned(),
            structure_type: structure_type.to_owned(),
            values: ols_structure_loads::line_values_path(file, structure),
            overrides: ols_structure_loads::overrides_path(file, Some(structure)),
        }
    }

    /// Before the application starts: a line-values file saying the
    /// values are being computed, unless one is there (its values shown
    /// until the new ones are written).
    pub fn prepare(&self) -> Result<(), String> {
        if self.values.is_file() {
            return Ok(());
        }
        self.pending("the line's values are being computed…")
    }

    /// A line-values file without values, saying why.
    pub fn pending(&self, why: &str) -> Result<(), String> {
        PlacedValues::pending(&self.structure, &self.line, &self.structure_type, why)
            .save(&self.values)
    }

    /// The line-values file from the line's structure loads `loads`.
    pub fn write(&self, loads: &[CaseLoads], line: &ols_line::Line) -> Result<(), String> {
        ols_structure_link::placed_values(loads, line, &self.structure, &self.line)
            .save(&self.values)
    }
}

/// The arguments of the structure application on structure file `path`:
/// the insulator assemblies, the angle that means ahead, the section
/// library and, for a placed structure, its id and line-values file.
pub fn arguments(
    path: &Path,
    insulators: Option<&Path>,
    ahead: f64,
    sections: Option<&Path>,
    placed: Option<&Placed>,
) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec![path.into(), "--ahead".into(), ahead.to_string().into()];
    if let Some(d) = insulators {
        a.extend(["--insulators".into(), d.into()]);
    }
    if let Some(s) = sections {
        a.extend(["--sections".into(), s.into()]);
    }
    if let Some(p) = placed {
        a.extend([
            "--structure".into(),
            p.structure.clone().into(),
            "--line-values".into(),
            p.values.clone().into(),
        ]);
    }
    a
}

/// What the line's structure loads are computed from: a copy of the
/// line, its cables, weather and load cases (taken on the main thread,
/// computed on another).
pub struct LineInputs {
    line: ols_line::Line,
    library: ols_cables::Library,
    weather: Vec<ols_weather::WeatherCase>,
    load_cases: Option<PathBuf>,
}

impl LineInputs {
    /// The model as it is now.
    pub fn of(model: &Model) -> Self {
        Self {
            line: model.line.clone(),
            library: model.library.clone(),
            weather: model.cases.iter().map(|c| c.case.clone()).collect(),
            load_cases: model.sources.load_cases.clone(),
        }
    }

    /// The structures' loads in every load case (as Checks › Structure
    /// loads computes them; seconds for a long line).
    pub fn compute(&self) -> Result<Vec<CaseLoads>, String> {
        let path = self.load_cases.as_ref().ok_or(
            "no load case file: Criteria › Load cases (a .olslc file next to the line, or the \
             project's own)",
        )?;
        let set = ols_loads::LoadCases::load(path)?;
        ols_loads::structure_loads(
            &self.line,
            &self.library,
            &self.weather,
            &set.cases,
            &ols_loads::default_budget(),
        )
    }

    /// Computes and writes the line-values file of each of `placed` (or
    /// one saying why there are none); what went wrong.
    pub fn write_all(&self, placed: &[Placed]) -> Vec<String> {
        let loads = self.compute();
        placed
            .iter()
            .filter_map(|p| {
                let r = match &loads {
                    Ok(l) => p.write(l, &self.line),
                    Err(e) => p
                        .pending(&format!("no values from the line: {e}"))
                        .and(Err(e.clone())),
                };
                r.err().map(|e| {
                    format!(
                        "loads of {} for the structure application: {e}",
                        p.structure
                    )
                })
            })
            .collect()
    }
}

/// The line's name for the header: the project line, else the line
/// file's name.
pub fn line_name(model: &Model) -> String {
    let l = model.project_line();
    if !l.is_empty() {
        return l;
    }
    let name = model
        .sources
        .file
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    name.strip_suffix(".ols.json")
        .map(str::to_owned)
        .unwrap_or(name)
}

/// **Edit…** on structure type `id`, asked from placed structure `placed`
/// (none: the Structure types page): the type's file and, for a placed
/// structure, what goes with it ([`StructureApps::launch`] prepares its
/// line-values file).
pub fn prepare_edit(
    model: &Model,
    id: &str,
    placed: Option<&str>,
) -> Result<(PathBuf, Option<Placed>), String> {
    let path = model
        .types
        .as_ref()
        .and_then(|(t, _)| t.path(id))
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("structure type {id}: no file read"))?;
    let placed = placed.map(|s| Placed::new(&path, s, &line_name(model), id));
    Ok((path, placed))
}

/// A change made in a structure application.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Structure file (path, structure id) saved.
    Structure(PathBuf, String),
    /// Values typed over the load table saved (file, the structure id or
    /// placed structure).
    Loads(PathBuf, String),
}

/// The program `name` (with the platform's suffix): next to `exe_dir`
/// (the running program's folder, or the folder above it: tests run from
/// `target/debug/deps`), else in a folder of `path` (the `PATH`).
pub fn find_in(exe_dir: Option<&Path>, path: Option<OsString>, name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let near = exe_dir
        .into_iter()
        .flat_map(|d| [Some(d.to_path_buf()), d.parent().map(Path::to_path_buf)])
        .flatten();
    let on_path = path
        .iter()
        .flat_map(std::env::split_paths)
        .collect::<Vec<_>>();
    near.chain(on_path)
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// The structure application: next to this program, else on the `PATH`.
pub fn find() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().ok();
    find_in(
        exe.as_deref().and_then(Path::parent),
        std::env::var_os("PATH"),
        PROGRAM,
    )
    .ok_or_else(|| {
        format!(
            "the structure application ({PROGRAM}) was found neither next to {} nor on the \
             PATH: install it next to ols-cad (cargo build -p ols-structure-app builds it)",
            exe.as_deref()
                .and_then(Path::parent)
                .map_or("this program".into(), |d| d.display().to_string())
        )
    })
}

/// A structure file being edited in the structure application.
struct Watched {
    /// The file.
    path: PathBuf,
    /// The structure's id (for messages).
    id: String,
    /// The application, while it runs.
    child: Option<Child>,
    /// The file's time and text when last read.
    modified: Option<SystemTime>,
    text: String,
    /// The placed structure whose loads it shows (none: the type alone).
    placed: Option<Placed>,
    /// The values typed over its load table: the file, its time and text
    /// when last read.
    overrides: (PathBuf, Option<SystemTime>, String),
}

/// The structure applications started, and their files.
#[derive(Default)]
pub struct StructureApps {
    watched: Vec<Watched>,
    last: Option<Instant>,
    /// The line's version the placed structures' values were last
    /// computed from.
    values_version: Option<u64>,
    /// The line's version seen last, and since when.
    seen: Option<(u64, Instant)>,
    /// A computation of the values running: what went wrong, when done.
    computing: Option<Receiver<Vec<String>>>,
}

impl StructureApps {
    /// Starts the structure application on `path` (structure `id`), with
    /// the insulator assemblies of `insulators`, the angle `ahead`, the
    /// section library `sections` and, on a placed structure, its line
    /// values (`placed`: the file prepared first, [`Placed::prepare`]);
    /// the files are watched from now on. One already running on the file
    /// is not started again.
    pub fn launch(
        &mut self,
        path: &Path,
        id: &str,
        insulators: Option<&Path>,
        ahead: f64,
        sections: Option<&Path>,
        placed: Option<Placed>,
    ) -> Result<(), String> {
        if self.watched.iter_mut().any(|w| {
            w.path == path
                && w.child
                    .as_mut()
                    .is_some_and(|c| c.try_wait().ok().flatten().is_none())
        }) {
            return Err(format!(
                "structure {id} is open in the structure application already"
            ));
        }
        if let Some(p) = &placed {
            p.prepare()?;
        }
        let program = find()?;
        let child = std::process::Command::new(&program)
            .args(arguments(
                path,
                insulators,
                ahead,
                sections,
                placed.as_ref(),
            ))
            .spawn()
            .map_err(|e| format!("{}: {e}", program.display()))?;
        let fresh = placed.is_some();
        self.watch(path, id, Some(child), placed);
        if fresh {
            // Computed for the new place at the next look.
            self.values_version = None;
            self.seen = None;
        }
        Ok(())
    }

    /// Watches `path` (structure `id`, `placed`: the placed structure),
    /// edited by `child` (none: by someone else).
    pub fn watch(&mut self, path: &Path, id: &str, child: Option<Child>, placed: Option<Placed>) {
        let (modified, text) = read(path);
        let at = placed.as_ref().map_or_else(
            || ols_structure_loads::overrides_path(path, None),
            |p| p.overrides.clone(),
        );
        let (m, t) = read(&at);
        self.watched.retain(|w| w.path != path);
        self.watched.push(Watched {
            path: path.to_path_buf(),
            id: id.to_owned(),
            child,
            modified,
            text,
            placed,
            overrides: (at, m, t),
        });
    }

    /// Whether a file is watched (the caller asks for a frame now and
    /// then).
    pub fn active(&self) -> bool {
        !self.watched.is_empty()
    }

    /// The placed structures whose loads are shown in a structure
    /// application.
    pub fn placed(&self) -> Vec<Placed> {
        self.watched
            .iter()
            .filter_map(|w| w.placed.clone())
            .collect()
    }

    /// The files saved since last looked at, at most every half second:
    /// structure files, and the values typed over a load table; files
    /// whose application has ended are looked at once more and dropped.
    pub fn poll(&mut self) -> Vec<Change> {
        if self.last.is_some_and(|t| t.elapsed() < POLL) {
            return Vec::new();
        }
        self.last = Some(Instant::now());
        let mut changed = Vec::new();
        for w in &mut self.watched {
            let modified = std::fs::metadata(&w.path).and_then(|m| m.modified()).ok();
            if modified != w.modified {
                let (m, text) = read(&w.path);
                w.modified = m;
                // Only a change of content (not a save of the same text).
                if !text.is_empty() && text != w.text {
                    w.text = text;
                    changed.push(Change::Structure(w.path.clone(), w.id.clone()));
                }
            }
            let (at, before, old) = &mut w.overrides;
            let modified = std::fs::metadata(&*at).and_then(|m| m.modified()).ok();
            if modified != *before {
                let (m, text) = read(at);
                *before = m;
                if text != *old {
                    *old = text;
                    let whose = w.placed.as_ref().map_or(&w.id, |p| &p.structure);
                    changed.push(Change::Loads(at.clone(), whose.clone()));
                }
            }
        }
        self.watched.retain_mut(|w| match &mut w.child {
            Some(c) => c.try_wait().ok().flatten().is_none(),
            None => true,
        });
        changed
    }

    /// While placed structures are open in a structure application: their
    /// line-values files written again when the line changed (once it has
    /// been unchanged for a second; computed on a thread of their own, one
    /// computation at a time). What went wrong in a computation finished.
    pub fn refresh_values(&mut self, model: &Model) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(rx) = &self.computing {
            match rx.try_recv() {
                Ok(errors) => {
                    out = errors;
                    self.computing = None;
                }
                Err(TryRecvError::Disconnected) => {
                    out.push("the line's values for the structure application: stopped".into());
                    self.computing = None;
                }
                Err(TryRecvError::Empty) => return out,
            }
        }
        let placed = self.placed();
        let version = model.line_version;
        if placed.is_empty() || self.values_version == Some(version) {
            return out;
        }
        // A new place is computed at once; a changed line once it settles.
        match self.seen {
            Some((v, since)) if v == version && since.elapsed() < SETTLE => return out,
            Some((v, _)) if v == version => {}
            _ if self.values_version.is_none() => {}
            _ => {
                self.seen = Some((version, Instant::now()));
                return out;
            }
        }
        self.seen = Some((version, Instant::now()));
        self.values_version = Some(version);
        let inputs = LineInputs::of(model);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(inputs.write_all(&placed));
        });
        self.computing = Some(rx);
        out
    }
}

/// The file's time and text (none and empty when it cannot be read).
fn read(path: &Path) -> (Option<SystemTime>, String) {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    (modified, std::fs::read_to_string(path).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::app::App;
    use crate::model::{self, Sources};
    use crate::selection::Selection;

    /// The structure application is found next to the program (or the
    /// folder above it), else on the PATH; nowhere: none.
    #[test]
    fn found_next_to_the_program_or_on_the_path() {
        let dir = std::env::temp_dir().join(format!("ols-cad-find-{}", std::process::id()));
        let (bin, deps, other) = (dir.join("bin"), dir.join("bin/deps"), dir.join("other"));
        for d in [&deps, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        let name = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(find_in(Some(&deps), None, PROGRAM), None);
        std::fs::write(other.join(&name), "").unwrap();
        let path = std::env::join_paths([other.clone()]).unwrap();
        assert_eq!(
            find_in(Some(&deps), Some(path.clone()), PROGRAM),
            Some(other.join(&name))
        );
        // Next to the program first (tests run one folder below it).
        std::fs::write(bin.join(&name), "").unwrap();
        assert_eq!(
            find_in(Some(&deps), Some(path), PROGRAM),
            Some(bin.join(&name))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Edit… on a structure with a type; its file, saved in the structure
    /// application (here: written), comes back into the line: the
    /// libraries are read again and the Structure types page shows the new
    /// text.
    #[test]
    fn a_structure_saved_there_comes_back_into_the_line() {
        use crate::library::LibraryKind;
        let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let ex = root.join("examples/synthetic-line");
        let dir =
            std::env::temp_dir().join(format!("ols-cad-structure-app-{}", std::process::id()));
        let structures = dir.join("Structures");
        std::fs::create_dir_all(&structures).unwrap();
        for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, structures.join(p.file_name().unwrap())).unwrap();
        }
        let model = model::load(
            Sources {
                file: ex.join("synthetic-line-typed.ols.json"),
                library: Some(root.join("libraries/cables")),
                structures: Some(structures.clone()),
                point_budget: 1000,
                corridor: 60.0,
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| App::with_model(model, None));
        harness.run();
        harness.state_mut().ready().unwrap().selection = Selection::Structure("T2".into());
        harness.run();
        // The button is there (not pressed: it would start the program).
        harness.get_by_label("Edit…");
        let path = structures.join("type-02.olss");
        let name = |h: &mut Harness<'_, App>| {
            let r = h.state_mut().ready().unwrap();
            let types = &r.model.types.as_ref().unwrap().0;
            types.get("type-02").unwrap().name.clone()
        };
        assert_eq!(name(&mut harness), "type-02 (2 structures, e.g. T2)");
        {
            let r = harness.state_mut().ready().unwrap();
            r.structure_apps.watch(&path, "type-02", None, None);
            r.edit_type("type-02");
        }
        harness.run();
        // Saved in the structure application.
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            text.replace(
                "name = \"type-02 (2 structures, e.g. T2)\"",
                "name = \"renamed\"",
            ),
        )
        .unwrap();
        let later = SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        std::thread::sleep(POLL + Duration::from_millis(100));
        harness.run();
        assert_eq!(name(&mut harness), "renamed");
        let r = harness.state_mut().ready().unwrap();
        let page = &r.libraries[&LibraryKind::StructureTypes];
        assert!(page.text().unwrap().contains("name = \"renamed\""));
        assert!(!page.dirty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Edit… on placed structure T2: the type's file, the arguments with
    /// the structure and its line-values file, the file prepared (being
    /// computed) and then written with T2's values in every load case
    /// (T4, of the same type, among the other places); values typed in
    /// the structure application (its overrides file written) come back
    /// as a change of T2's loads.
    #[test]
    fn edit_on_a_placed_structure_hands_over_its_line_values() {
        let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let ex = root.join("examples/synthetic-line");
        let dir = std::env::temp_dir().join(format!("ols-cad-placed-{}", std::process::id()));
        let structures = dir.join("Structures");
        std::fs::create_dir_all(&structures).unwrap();
        for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, structures.join(p.file_name().unwrap())).unwrap();
        }
        let mut model = model::load(
            Sources {
                file: ex.join("synthetic-line-typed.ols.json"),
                library: Some(root.join("libraries/cables")),
                structures: Some(structures.clone()),
                weather: Some(ex.join("weather-cases.olswc")),
                load_cases: Some(ex.join("load-cases.olslc")),
                point_budget: 1000,
                corridor: 60.0,
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap();
        let (path, placed) = prepare_edit(&model, "type-02", Some("T2")).unwrap();
        assert_eq!(path, structures.join("type-02.olss"));
        let placed = placed.unwrap();
        assert_eq!(
            placed.values,
            structures.join("type-02.T2.line-values.json")
        );
        assert_eq!(placed.overrides, structures.join("type-02.T2.loads.csv"));
        assert_eq!(placed.line, "synthetic-line-typed");
        let args = arguments(&path, None, 90.0, Some(&dir), Some(&placed));
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let after = |flag: &str| {
            let k = args.iter().position(|a| a == flag).unwrap();
            args[k + 1].clone()
        };
        assert_eq!(after("--structure"), "T2");
        assert_eq!(after("--line-values"), placed.values.to_string_lossy());
        assert_eq!(after("--sections"), dir.to_string_lossy());
        // Prepared: the file is there before the application starts.
        placed.prepare().unwrap();
        let v = PlacedValues::load(&placed.values).unwrap();
        assert_eq!((v.structure.as_str(), v.values.values.len()), ("T2", 0));
        assert!(v.note.is_some());
        // Computed (on a thread of its own in the application), with the
        // creep stretch of the example (as the core's integration test).
        model.line.creep_stretch.creep_case = Some("everyday".into());
        assert!(
            LineInputs::of(&model)
                .write_all(std::slice::from_ref(&placed))
                .is_empty()
        );
        let v = PlacedValues::load(&placed.values).unwrap();
        assert_eq!(v.note, None);
        assert_eq!(v.structure_type, "type-02");
        assert_eq!(v.cases.len(), 3);
        let labels: Vec<&str> = v.values.points.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["A1", "A11"]);
        let cases: std::collections::BTreeSet<&str> =
            v.values.values.iter().map(|x| x.case.as_str()).collect();
        assert_eq!(
            v.values.values.len(),
            2 * 3 * 6,
            "{cases:?} of {:?}",
            v.cases
        );
        assert!(v.others.iter().any(|o| o.structure == "T4"));
        // Typed loads saved there: a change of T2's loads.
        let mut apps = StructureApps::default();
        apps.watch(&path, "type-02", None, Some(placed.clone()));
        assert_eq!(apps.placed(), std::slice::from_ref(&placed));
        std::fs::write(&placed.overrides, "case,load\n").unwrap();
        let later = SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&placed.overrides)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(
            apps.poll(),
            [Change::Loads(placed.overrides.clone(), "T2".into())]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
