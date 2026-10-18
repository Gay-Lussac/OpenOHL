// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! **Edit** on a structure: the structure application (`ohl-structure`,
//! OHL-Structure; under its former name `ols-structure` when that is all
//! there is)
//! started on the structure's file (in a project, its working copy), and
//! the file watched: when it is saved there, the line application stores
//! it into the project and reads the libraries again (as a save of the
//! Structure types page does). The file's time is looked at twice a
//! second at most, only while a structure application runs or a change is
//! waiting. The library folders it was given (the structure file's,
//! `--insulators`, `--sections`) are watched too: library files imported
//! there (its File › Libraries) are stored into the project
//! ([`Change::Libraries`], [`library_signature`]).
//!
//! On a **placed structure** (Properties › Edit…) the application gets
//! that place's loads: the line's values there in the cases the line
//! sends to its structures (Criteria › Loads for structures: the load
//! cases, all by default, then weather cases chosen on their own with all
//! factors 1; `ohl_structure_link::structure_cases`,
//! `ohl_structure_link::placed_values`) written beside the structure
//! file as `<stem>.<structure>.line-values.json`
//! (`ohl_structure_loads::line_values_path`) and passed with
//! `--line-values FILE --structure ID`; written again (in the background)
//! when the line changes while the application runs, and read again
//! there. The values typed over its load table come back as
//! `<stem>.<structure>.loads.csv` and are kept in the project (library
//! kind `loads`, beside the structure types) when they change. The
//! project's section library goes along as `--sections`.
//!
//! What goes wrong is shown in a window ([`report_window`]) and logged:
//! the program not found (where it was looked for, how to build it); of
//! another build (`ohl-structure --version` compared with [`VERSION`]: a
//! warning, started anyway); stopped with an error (its error output,
//! read on a thread of its own).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use ohl_structure_link::StructureCaseLoads;
use ohl_structure_loads::PlacedValues;

use crate::model::Model;

/// The structure application's program name.
pub const PROGRAM: &str = "ohl-structure";

/// The structure application's former program name (before OpenOHL),
/// still looked for when [`PROGRAM`] is not found.
pub const LEGACY_PROGRAM: &str = "ols-structure";

/// This program's version: the workspace version and the build id (the
/// git commit built from, `build.rs`), as `--version` prints it. The
/// structure application's (`ohl-structure --version`) is compared with
/// it before it is started.
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (build ",
    env!("OHL_BUILD_ID"),
    ")"
);

/// The environment variable naming the structure application's program
/// (looked at before the folders, [`find`]).
pub const PROGRAM_ENV: &str = "OHL_STRUCTURE_APP";

/// The variable's former name, still read when [`PROGRAM_ENV`] is not
/// set.
pub const LEGACY_PROGRAM_ENV: &str = "OLS_STRUCTURE_APP";

/// How the structure application is built.
pub const BUILD: &str = "cargo build --release -p ohl-structure-app";

/// How both applications are built from the same code.
pub const BUILD_BOTH: &str = "cargo build --release -p ohl-app -p ohl-structure-app";

/// A structure application that stops this soon after its start did not
/// start (its error output says why).
const EARLY: Duration = Duration::from_secs(5);

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
            values: ohl_structure_loads::line_values_path(file, structure),
            overrides: ohl_structure_loads::overrides_path(file, Some(structure)),
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

    /// The line-values file from the line's structure loads `loads` in
    /// the cases it sends to its structures.
    pub fn write(&self, loads: &StructureCaseLoads, line: &ohl_line::Line) -> Result<(), String> {
        ohl_structure_link::placed_values(
            &loads.load,
            &loads.weather,
            &loads.missing,
            line,
            &self.structure,
            &self.line,
        )
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

/// Why a line without load cases and without weather cases chosen for the
/// structures sends no values.
pub const NO_CASES: &str = "the line has no load cases and no weather case is chosen for the \
    structures: tick weather cases in Criteria › Loads for structures (each sent with all \
    factors 1), or give load cases (Criteria › Load cases library, Criteria › Design code)";

/// What the line's structure loads are computed from: a copy of the
/// line, its cables, weather and load cases (taken on the main thread,
/// computed on another).
pub struct LineInputs {
    line: ohl_line::Line,
    library: ohl_cables::Library,
    weather: Vec<ohl_weather::WeatherCase>,
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

    /// The structures' loads in the cases the line sends to them
    /// (Criteria › Loads for structures: its load cases, all by default,
    /// then the weather cases chosen on their own), as Checks › Structure
    /// loads computes them (seconds for a long line). Err: no case to send
    /// (no load case file and no weather case chosen, or none chosen).
    pub fn compute(&self) -> Result<StructureCaseLoads, String> {
        let load_cases = match &self.load_cases {
            Some(path) => ohl_loads::LoadCases::load(path)?.cases,
            None => Vec::new(),
        };
        let names: Vec<String> = self.weather.iter().map(|w| w.name.clone()).collect();
        let cases =
            ohl_structure_link::structure_cases(&load_cases, &self.line.structure_loads, &names);
        if cases.is_empty() {
            return Err(if self.load_cases.is_none() {
                NO_CASES.into()
            } else {
                "no case chosen for the structures: Criteria › Loads for structures".into()
            });
        }
        ohl_structure_link::structure_case_loads(
            &self.line,
            &self.library,
            &self.weather,
            cases,
            &ohl_loads::default_budget(),
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
                r.err()
                    .map(|e| format!("loads of {} for OHL-Structure: {e}", p.structure))
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
    ohl_core::files::line_file_stem(&name)
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
    /// Library files imported (or changed) in a folder the application
    /// was given (the folder, the structure id).
    Libraries(PathBuf, String),
}

/// The folders the program is looked for in: next to `exe_dir` (the
/// running program's folder, and the folder above it: tests run from
/// `target/debug/deps`), then the folders of `path` (the `PATH`).
pub fn places(exe_dir: Option<&Path>, path: Option<OsString>) -> Vec<PathBuf> {
    let near = exe_dir
        .into_iter()
        .flat_map(|d| [Some(d.to_path_buf()), d.parent().map(Path::to_path_buf)])
        .flatten();
    let on_path = path
        .iter()
        .flat_map(std::env::split_paths)
        .collect::<Vec<_>>();
    near.chain(on_path).collect()
}

/// The program `name` (with the platform's suffix) in the first of
/// [`places`] that has it.
pub fn find_in(exe_dir: Option<&Path>, path: Option<OsString>, name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    places(exe_dir, path)
        .into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// The structure application: the program [`PROGRAM_ENV`] (or, by its
/// former name, [`LEGACY_PROGRAM_ENV`]) names, else [`find_either`] next
/// to this program or on the `PATH`. Not found: where it was looked for
/// and how to build it.
pub fn find() -> Result<PathBuf, String> {
    for var in [PROGRAM_ENV, LEGACY_PROGRAM_ENV] {
        if let Some(p) = std::env::var_os(var).filter(|p| !p.is_empty()) {
            let p = PathBuf::from(p);
            return if p.is_file() {
                Ok(p)
            } else {
                Err(format!(
                    "{var} names {}, but there is no such file.\n\nBuild the structure \
                     application (in the OpenOHL folder):\n    {BUILD}\nor unset {var}.",
                    p.display()
                ))
            };
        }
    }
    let exe = std::env::current_exe().ok();
    let dir = exe.as_deref().and_then(Path::parent);
    let path = std::env::var_os("PATH");
    find_either(dir, path.clone()).ok_or_else(|| not_found(&places(dir, path)))
}

/// The structure application under its name ([`PROGRAM`]) in any of
/// [`places`], else under its former name ([`LEGACY_PROGRAM`]).
pub fn find_either(exe_dir: Option<&Path>, path: Option<OsString>) -> Option<PathBuf> {
    find_in(exe_dir, path.clone(), PROGRAM).or_else(|| find_in(exe_dir, path, LEGACY_PROGRAM))
}

/// Why the structure application was not found: where it was looked for,
/// how to build it.
pub fn not_found(places: &[PathBuf]) -> String {
    let file = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    let list: Vec<String> = places
        .iter()
        .map(|d| format!("    {}", d.display()))
        .collect();
    format!(
        "OHL-Structure ({file}) was not found.\n\nLooked in:\n{}\n\nBuild it (in \
         the OpenOHL folder):\n    {BUILD}\nIt is then next to ohl-design (target/release). Or set \
         {PROGRAM_ENV} to its path.",
        list.join("\n")
    )
}

/// What `program --version` prints after the program's name (e.g.
/// `0.0.1 (build 1a2b3c4d5e6f)`); waited for 3 s at most.
pub fn version_of(program: &Path) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} --version did not answer within 3 s",
                    program.display()
                ));
            }
            Err(e) => return Err(format!("{}: {e}", program.display())),
        }
    };
    let mut out = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut out);
    }
    if !status.success() {
        return Err(format!("{} --version: {status}", program.display()));
    }
    let line = out.lines().next().unwrap_or("").trim();
    Ok(line
        .strip_prefix(PROGRAM)
        .or_else(|| line.strip_prefix(LEGACY_PROGRAM))
        .map_or(line, str::trim)
        .to_owned())
}

/// The warning when the structure application (its version `theirs`, as
/// [`version_of`] reads it) is not of this program's build; none when
/// they match, or when the build id is not known on either side and the
/// version numbers match.
pub fn version_warning(program: &Path, theirs: &str) -> Option<String> {
    if theirs == VERSION {
        return None;
    }
    let number = |v: &str| v.split_whitespace().next().unwrap_or("").to_owned();
    let unknown = |v: &str| v.contains("(build unknown)");
    if (unknown(theirs) || unknown(VERSION)) && number(theirs) == number(VERSION) {
        return None;
    }
    Some(format!(
        "OHL-Structure is from another build: rebuild both (in the OpenOHL \
         folder):\n    {BUILD_BOTH}\n\nThis program (ohl-design): {VERSION}\nThe structure \
         application ({}): {}\n\nIt was started anyway; what it saves may not read the same \
         here.",
        program.display(),
        if theirs.is_empty() {
            "no version"
        } else {
            theirs
        }
    ))
}

/// A report of the structure application: shown in a window of its own
/// (the text in a box of fixed height) until closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The window's title.
    pub title: String,
    /// What happened.
    pub text: String,
    /// An error (else a warning).
    pub error: bool,
}

/// The structure application's error output, read on a thread of its own
/// (so the pipe never fills and nothing waits for it): each line passed
/// on to this program's error output, the last ones kept.
struct Stderr {
    text: Arc<Mutex<String>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// The most of the error output kept, bytes.
const STDERR_KEPT: usize = 64 * 1024;

impl Stderr {
    fn read(pipe: impl std::io::Read + Send + 'static) -> Self {
        use std::io::BufRead;
        let text = Arc::new(Mutex::new(String::new()));
        let kept = Arc::clone(&text);
        let thread = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(pipe);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let l = String::from_utf8_lossy(&line);
                        eprint!("{l}");
                        if let Ok(mut t) = kept.lock() {
                            t.push_str(&l);
                            if t.len() > STDERR_KEPT {
                                let mut cut = t.len() - STDERR_KEPT;
                                while !t.is_char_boundary(cut) {
                                    cut += 1;
                                }
                                t.drain(..cut);
                            }
                        }
                    }
                }
            }
        });
        Self {
            text,
            thread: Some(thread),
        }
    }

    /// What was read (the program has ended: the rest is waited for a
    /// moment at most).
    fn finish(&mut self) -> String {
        if let Some(t) = self.thread.take() {
            let until = Instant::now() + Duration::from_millis(300);
            while !t.is_finished() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
        self.text.lock().map(|t| t.clone()).unwrap_or_default()
    }
}

/// A structure application started.
struct Running {
    child: Child,
    started: Instant,
    stderr: Option<Stderr>,
}

/// The names of a file's load cases, or why they could not be read.
type CaseNames = Result<Vec<String>, String>;

/// A structure file being edited in the structure application.
struct Watched {
    /// The file.
    path: PathBuf,
    /// The structure's id (for messages).
    id: String,
    /// The application, while it runs.
    child: Option<Running>,
    /// The file's time and text when last read.
    modified: Option<SystemTime>,
    text: String,
    /// The placed structure whose loads it shows (none: the type alone).
    placed: Option<Placed>,
    /// The values typed over its load table: the file, its time and text
    /// when last read.
    overrides: (PathBuf, Option<SystemTime>, String),
    /// The library folders the application may import into (the
    /// structure file's folder, its insulators and sections) and what was
    /// in them when last looked ([`library_signature`]).
    libraries: (Vec<PathBuf>, u64),
}

/// The library files the structure application imports (its
/// File › Libraries): section libraries, insulator assemblies, poles,
/// structure models, load-case recipes, pylon templates.
pub const LIBRARY_EXTENSIONS: [&str; 6] = ["ohlsec", "ohli", "ohlp", "ohlfe", "ohlslc", "ohlpt"];

/// What the library files in `dirs` are: their names, sizes and times
/// hashed (a file added, changed or removed changes it).
pub fn library_signature(dirs: &[PathBuf]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for d in dirs {
        let mut files: Vec<(String, u64, Option<SystemTime>)> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| {
                LIBRARY_EXTENSIONS
                    .iter()
                    .any(|l| ohl_core::files::has_extension(&e.path(), l))
            })
            .map(|e| {
                let m = e.metadata().ok();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    m.as_ref().map_or(0, std::fs::Metadata::len),
                    m.and_then(|m| m.modified().ok()),
                )
            })
            .collect();
        files.sort();
        d.hash(&mut h);
        files.hash(&mut h);
    }
    h.finish()
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
    /// The program started (none: [`find`] it).
    pub program: Option<PathBuf>,
    /// The programs whose version was checked (path, time of the file).
    checked: Vec<(PathBuf, Option<SystemTime>)>,
    /// What went wrong (or is worth a warning), shown in windows until
    /// closed ([`report_window`]).
    pub reports: Vec<Report>,
    /// Tests: the arguments of each launch, recorded instead of starting
    /// the program.
    #[cfg(test)]
    pub dry_run: Option<Vec<Vec<OsString>>>,
    /// The names of the load cases (Criteria › Loads for structures): the
    /// file, its time when read, its cases' names or why not.
    load_case_names: Option<(PathBuf, Option<SystemTime>, CaseNames)>,
}

impl StructureApps {
    /// The names of the load cases of `path` (the line's `.ohllc`), read
    /// again when the file changed.
    pub fn load_case_names(&mut self, path: &Path) -> Result<Vec<String>, String> {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        match &self.load_case_names {
            Some((p, m, names)) if p == path && *m == modified => names.clone(),
            _ => {
                let names = ohl_loads::LoadCases::load(path)
                    .map(|set| set.cases.into_iter().map(|c| c.name).collect::<Vec<_>>());
                self.load_case_names = Some((path.to_path_buf(), modified, names.clone()));
                names
            }
        }
    }

    /// Starts the structure application on `path` (structure `id`), with
    /// the insulator assemblies of `insulators`, the angle `ahead`, the
    /// section library `sections` and, on a placed structure, its line
    /// values (`placed`: the file prepared first, [`Placed::prepare`]);
    /// the files are watched from now on. One already running on the file
    /// is not started again. Not found, or not started: a report too
    /// ([`StructureApps::reports`]); of another build: a warning, started
    /// anyway. Its error output is read, and shown when it stops with an
    /// error.
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
                    .is_some_and(|c| c.child.try_wait().ok().flatten().is_none())
        }) {
            log::info!("structure {id} is open in the structure application already");
            return Err(format!("structure {id} is open in OHL-Structure already"));
        }
        if let Some(p) = &placed {
            p.prepare()?;
        }
        let args = arguments(path, insulators, ahead, sections, placed.as_ref());
        #[cfg(test)]
        if let Some(dry) = &mut self.dry_run {
            dry.push(args);
            return Ok(());
        }
        let program = match self.program.clone().map_or_else(find, Ok) {
            Ok(p) => p,
            Err(e) => {
                log::info!("structure application not found: {e}");
                self.reports.push(Report {
                    title: "OHL-Structure was not found".into(),
                    text: e,
                    error: true,
                });
                return Err(format!("OHL-Structure ({PROGRAM}) was not found: {BUILD}"));
            }
        };
        if program.is_file() {
            self.check_version(&program);
        }
        let shown: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        log::info!(
            "starting OHL-Structure: {} {}",
            program.display(),
            shown.join(" ")
        );
        let spawned = std::process::Command::new(&program)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let text = format!(
                    "{} could not be started: {e}\n\nArguments: {}\n\nBuild it again (in the \
                     OpenOHL folder):\n    {BUILD}",
                    program.display(),
                    shown.join(" ")
                );
                log::info!("{text}");
                self.reports.push(Report {
                    title: "OHL-Structure did not start".into(),
                    text,
                    error: true,
                });
                return Err(format!("{}: {e}", program.display()));
            }
        };
        let stderr = child.stderr.take().map(Stderr::read);
        let fresh = placed.is_some();
        // The folders it imports libraries into: given, else its defaults
        // beside the file (`Insulators/`, `Sections/`).
        let beside = path.parent().unwrap_or(Path::new("."));
        let folder = |p: &Path| {
            if p.is_file() {
                p.parent().unwrap_or(beside).to_path_buf()
            } else {
                p.to_path_buf()
            }
        };
        let extra = vec![
            insulators.map_or_else(|| beside.join("Insulators"), folder),
            sections.map_or_else(|| beside.join("Sections"), folder),
        ];
        self.watch_running(
            path,
            id,
            Some(Running {
                child,
                started: Instant::now(),
                stderr,
            }),
            placed,
            extra,
        );
        if fresh {
            // Computed for the new place at the next look.
            self.values_version = None;
            self.seen = None;
        }
        Ok(())
    }

    /// Once per program file (until it is built again): its version
    /// compared with this program's; another build is a warning.
    fn check_version(&mut self, program: &Path) {
        let modified = std::fs::metadata(program).and_then(|m| m.modified()).ok();
        let key = (program.to_path_buf(), modified);
        if self.checked.contains(&key) {
            return;
        }
        self.checked.push(key);
        let warning = match version_of(program) {
            Ok(v) => {
                log::info!("structure application {}: {v}", program.display());
                version_warning(program, &v)
            }
            Err(e) => {
                log::info!("structure application version: {e}");
                Some(format!(
                    "Its version could not be read ({e}): it may be an older build. Rebuild both \
                     (in the OpenOHL folder):\n    {BUILD_BOTH}\n\nIt was started anyway."
                ))
            }
        };
        if let Some(text) = warning {
            log::info!("structure application of another build (here {VERSION})");
            self.reports.push(Report {
                title: "OHL-Structure is from another build".into(),
                text,
                error: false,
            });
        }
    }

    /// Watches `path` (structure `id`, `placed`: the placed structure),
    /// edited by `child` (none: by someone else). Tests: the file saved
    /// elsewhere.
    #[cfg(test)]
    pub fn watch(&mut self, path: &Path, id: &str, child: Option<Child>, placed: Option<Placed>) {
        let running = child.map(|child| Running {
            child,
            started: Instant::now(),
            stderr: None,
        });
        self.watch_running(path, id, running, placed, Vec::new());
    }

    /// Watches `path` as [`StructureApps::watch`], and the library folders
    /// `libraries` besides the file's own.
    #[cfg(test)]
    pub fn watch_libraries(&mut self, path: &Path, id: &str, libraries: Vec<PathBuf>) {
        self.watch_running(path, id, None, None, libraries);
    }

    fn watch_running(
        &mut self,
        path: &Path,
        id: &str,
        child: Option<Running>,
        placed: Option<Placed>,
        extra: Vec<PathBuf>,
    ) {
        let mut dirs = vec![path.parent().unwrap_or(Path::new(".")).to_path_buf()];
        for d in extra {
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
        let signature = library_signature(&dirs);
        let (modified, text) = read(path);
        let at = placed.as_ref().map_or_else(
            || ohl_structure_loads::overrides_path(path, None),
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
            libraries: (dirs, signature),
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
            let (dirs, before) = &mut w.libraries;
            let now = library_signature(dirs);
            if now != *before {
                *before = now;
                changed.push(Change::Libraries(dirs[0].clone(), w.id.clone()));
            }
        }
        let mut reports = Vec::new();
        self.watched.retain_mut(|w| {
            let Some(r) = &mut w.child else {
                return true;
            };
            let status = match r.child.try_wait() {
                Ok(None) => return true,
                Ok(Some(s)) => s,
                Err(e) => {
                    log::info!("structure application on {}: {e}", w.id);
                    return false;
                }
            };
            let after = r.started.elapsed();
            let stderr = r.stderr.as_mut().map(Stderr::finish).unwrap_or_default();
            if status.success() {
                log::info!("structure application on {} closed", w.id);
            } else {
                log::info!(
                    "structure application on {} stopped after {:.1} s: {status}",
                    w.id,
                    after.as_secs_f64()
                );
                reports.push(stopped(&w.id, &status.to_string(), after, &stderr));
            }
            false
        });
        self.reports.extend(reports);
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
                    out.push("the line's values for OHL-Structure: stopped".into());
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

/// The report of a structure application (on structure `id`) that stopped
/// with `status` after `after`, with its error output `stderr`.
pub fn stopped(id: &str, status: &str, after: Duration, stderr: &str) -> Report {
    let early = after < EARLY;
    let mut text = format!(
        "OHL-Structure on {id} stopped {} ({status}).",
        if early {
            format!("right after it started ({:.1} s)", after.as_secs_f64())
        } else {
            format!("after {:.0} s", after.as_secs_f64())
        }
    );
    if stderr.trim().is_empty() {
        text.push_str("\n\nIt wrote no error output.");
    } else {
        text.push_str("\n\nIts error output:\n");
        text.push_str(stderr.trim_end());
    }
    if early {
        text.push_str(&format!(
            "\n\nAn older build may not read the file: rebuild both (in the OpenOHL \
             folder):\n    {BUILD_BOTH}"
        ));
    }
    Report {
        title: if early {
            "OHL-Structure did not start".into()
        } else {
            "OHL-Structure stopped with an error".into()
        },
        text,
        error: true,
    }
}

/// The reports of `apps`, a window each (the text in a box of fixed
/// height, selectable to copy), until closed.
pub fn report_window(ctx: &egui::Context, apps: &mut StructureApps) {
    let mut close = None;
    for (k, r) in apps.reports.iter().enumerate() {
        let mut open = true;
        egui::Window::new(&r.title)
            .id(egui::Id::new(("structure-app-report", k)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                let color = if r.error {
                    ui.visuals().error_fg_color
                } else {
                    ui.visuals().warn_fg_color
                };
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("structure-app-report-text", k))
                        .max_height(crate::menu::TEXT_BOX_HEIGHT)
                        .min_scrolled_height(crate::menu::TEXT_BOX_HEIGHT)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&r.text).color(color))
                                    .selectable(true),
                            );
                        });
                });
                if ui.button("Close").clicked() {
                    close = Some(k);
                }
            });
        if !open {
            close = Some(k);
        }
    }
    if let Some(k) = close {
        apps.reports.remove(k);
    }
}

/// Criteria › Loads for structures: what the line sends to the placed
/// structures (the cases of the structure application): its load cases
/// (all by default) and weather cases on their own, all factors 1 (none
/// by default). A tick menu (it stays open while ticking); each tick is
/// one undoable edit of the line's settings (`structure_loads`).
pub fn loads_menu(ui: &mut egui::Ui, r: &mut crate::app::Ready) {
    crate::menu::options_menu(ui, "Loads for structures", |ui| {
        ui.set_max_width(420.0);
        ui.label(
            egui::RichText::new(
                "What the line sends to the placed structures (Edit… on a placed structure: \
                 the cases of OHL-Structure). Load cases (Criteria › Load cases library, \
                 Criteria › Design code) add factors and combinations on top of the weather \
                 cases; a weather case on its own is sent with all factors 1, the wires as \
                 strung.",
            )
            .small()
            .weak(),
        );
        let before = r.model.line.structure_loads.clone();
        let mut choice = before.clone();
        let editable = r.model.can_edit();
        if let Err(why) = &editable {
            crate::menu::error_label(ui, format!("read-only: {why}"));
        }
        ui.add_enabled_ui(editable.is_ok(), |ui| {
            ui.separator();
            ui.label(egui::RichText::new("Load cases").strong());
            let names = match r.model.sources.load_cases.clone() {
                None => {
                    ui.label(
                        egui::RichText::new("no load case file (Criteria › Load cases library)")
                            .weak(),
                    );
                    Vec::new()
                }
                Some(path) => match r.structure_apps.load_case_names(&path) {
                    Ok(n) => n,
                    Err(e) => {
                        crate::menu::error_label(ui, e);
                        Vec::new()
                    }
                },
            };
            if !names.is_empty() {
                ui.horizontal(|ui| {
                    if ui
                        .button("all")
                        .on_hover_text("Every load case, also those added later (the default)")
                        .clicked()
                    {
                        choice.load_cases = None;
                    }
                    if ui.button("none").clicked() {
                        choice.load_cases = Some(Vec::new());
                    }
                });
            }
            // The chosen not in the file: shown, to be unticked.
            let gone: Vec<String> = choice
                .load_cases
                .iter()
                .flatten()
                .filter(|n| !names.contains(n))
                .cloned()
                .collect();
            for name in names.iter().chain(&gone) {
                let mut on = choice.sends_load_case(name);
                let text = if gone.contains(name) {
                    format!("{name} (not in the load cases)")
                } else {
                    name.clone()
                };
                if ui.checkbox(&mut on, text).changed() {
                    let mut list: Vec<String> =
                        choice.load_cases.clone().unwrap_or_else(|| names.clone());
                    list.retain(|n| n != name);
                    if on {
                        list.push(name.clone());
                    }
                    // In the file's order; every one of them: all.
                    list.sort_by_key(|n| names.iter().position(|x| x == n).unwrap_or(usize::MAX));
                    choice.load_cases = (!names.iter().all(|n| list.contains(n))
                        || list.len() != names.len())
                    .then_some(list);
                }
            }
            ui.separator();
            ui.label(egui::RichText::new("Weather cases (all factors 1)").strong());
            if ui.button("none").clicked() {
                choice.weather_cases.clear();
            }
            let weather: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
            let gone: Vec<String> = choice
                .weather_cases
                .iter()
                .filter(|n| !weather.contains(n))
                .cloned()
                .collect();
            for name in weather.iter().chain(&gone) {
                let mut on = choice.weather_cases.contains(name);
                let text = if gone.contains(name) {
                    format!("{name} (not in the weather cases)")
                } else {
                    name.clone()
                };
                if ui.checkbox(&mut on, text).changed() {
                    choice.weather_cases.retain(|n| n != name);
                    if on {
                        choice.weather_cases.push(name.clone());
                        choice.weather_cases.sort_by_key(|n| {
                            weather.iter().position(|x| x == n).unwrap_or(usize::MAX)
                        });
                    }
                }
            }
            if names.is_empty() && choice.weather_cases.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No load case and no weather case: the structures get no values from \
                         the line.",
                    )
                    .small()
                    .color(ui.visuals().warn_fg_color),
                );
            }
        });
        if choice != before
            && let Ok(e) = &r.model.editing
        {
            let command = e.session.structure_loads_command(choice);
            r.perform(crate::edit::Action::Command {
                what: "loads for structures".into(),
                command,
            });
        }
    });
}

/// The file's time and text (none and empty when it cannot be read).
fn read(path: &Path) -> (Option<SystemTime>, String) {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    (modified, std::fs::read_to_string(path).unwrap_or_default())
}

#[cfg(test)]
mod tests {
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
        let dir = std::env::temp_dir().join(format!("ohl-design-find-{}", std::process::id()));
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

    /// The program under its former name is found when the new name is
    /// nowhere; the new name wins wherever it is.
    #[test]
    fn the_former_name_is_found_after_the_new_one() {
        let dir = std::env::temp_dir().join(format!("ohl-design-legacy-{}", std::process::id()));
        let (bin, other) = (dir.join("bin"), dir.join("other"));
        for d in [&bin, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        let suffix = std::env::consts::EXE_SUFFIX;
        let (new, old) = (
            format!("{PROGRAM}{suffix}"),
            format!("{LEGACY_PROGRAM}{suffix}"),
        );
        let path = std::env::join_paths([other.clone()]).unwrap();
        assert_eq!(find_either(Some(&bin), Some(path.clone())), None);
        std::fs::write(bin.join(&old), "").unwrap();
        assert_eq!(
            find_either(Some(&bin), Some(path.clone())),
            Some(bin.join(&old))
        );
        // The new name on the PATH comes before the old one next to it.
        std::fs::write(other.join(&new), "").unwrap();
        assert_eq!(find_either(Some(&bin), Some(path)), Some(other.join(&new)));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A section library or insulator assembly imported in the structure
    /// application (into the folders it was given, or beside the file) is
    /// a change of the libraries; other files there are not.
    #[test]
    fn libraries_imported_there_are_a_change() {
        let dir = std::env::temp_dir().join(format!("ohl-design-libs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (structures, sections) = (dir.join("structures"), dir.join("sections"));
        std::fs::create_dir_all(&structures).unwrap();
        std::fs::create_dir_all(&sections).unwrap();
        let path = structures.join("t.ohls");
        std::fs::write(&path, "id = \"t\"\n").unwrap();
        let mut apps = StructureApps::default();
        apps.watch_libraries(&path, "t", vec![sections.clone()]);
        assert!(apps.poll().is_empty());
        // Not a library file: nothing.
        std::fs::write(structures.join("t.line-values.json"), "{}").unwrap();
        std::thread::sleep(POLL + Duration::from_millis(50));
        assert!(apps.poll().is_empty());
        std::fs::write(sections.join("mine.ohlsec"), "format = \"ohl-sections\"\n").unwrap();
        std::thread::sleep(POLL + Duration::from_millis(50));
        assert_eq!(
            apps.poll(),
            vec![Change::Libraries(structures.clone(), "t".into())]
        );
        std::fs::write(structures.join("p.ohlp"), "id = \"p\"\n").unwrap();
        std::thread::sleep(POLL + Duration::from_millis(50));
        assert_eq!(apps.poll().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
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
            std::env::temp_dir().join(format!("ohl-design-structure-app-{}", std::process::id()));
        let structures = dir.join("Structures");
        std::fs::create_dir_all(&structures).unwrap();
        for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, structures.join(p.file_name().unwrap())).unwrap();
        }
        let model = model::load(
            Sources {
                file: ex.join("synthetic-line-typed.ohl.json"),
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
        {
            let r = harness.state_mut().ready().unwrap();
            r.selection = Selection::Structure("T2".into());
            // The arguments recorded (the program is not started).
            r.structure_apps.dry_run = Some(Vec::new());
        }
        harness.run();
        harness
            .get_by_label(crate::properties::OPEN_IN_STRUCTURE_APP)
            .click();
        harness.run();
        let path = structures.join("type-02.ohls");
        {
            let r = harness.state_mut().ready().unwrap();
            let launched = r.structure_apps.dry_run.take().unwrap();
            assert_eq!(launched.len(), 1, "{:?}", r.status);
            let args: Vec<String> = launched[0]
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            assert_eq!(args[0], path.to_string_lossy());
            let after = |flag: &str| {
                let k = args.iter().position(|a| a == flag).unwrap();
                args[k + 1].clone()
            };
            assert_eq!(after("--structure"), "T2");
            assert_eq!(
                after("--line-values"),
                structures
                    .join("type-02.T2.line-values.json")
                    .to_string_lossy()
            );
            assert_eq!(after("--ahead"), "0");
            assert!(args.iter().any(|a| a == "--insulators"));
            assert!(args.iter().any(|a| a == "--sections"));
        }
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
        let dir = std::env::temp_dir().join(format!("ohl-design-placed-{}", std::process::id()));
        let structures = dir.join("Structures");
        std::fs::create_dir_all(&structures).unwrap();
        for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, structures.join(p.file_name().unwrap())).unwrap();
        }
        let mut model = model::load(
            Sources {
                file: ex.join("synthetic-line-typed.ohl.json"),
                library: Some(root.join("libraries/cables")),
                structures: Some(structures.clone()),
                weather: Some(ex.join("weather-cases.ohlwc")),
                load_cases: Some(ex.join("load-cases.ohllc")),
                point_budget: 1000,
                corridor: 60.0,
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap();
        let (path, placed) = prepare_edit(&model, "type-02", Some("T2")).unwrap();
        assert_eq!(path, structures.join("type-02.ohls"));
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
        assert_eq!(
            v.sources,
            [ohl_structure_loads::CaseSource::LoadCase; 3],
            "every load case by default, no weather case"
        );
        // A weather case chosen on its own: sent after the load cases.
        let mut inputs = LineInputs::of(&model);
        inputs.line.structure_loads.weather_cases = vec!["glaze ice".into()];
        assert!(inputs.write_all(std::slice::from_ref(&placed)).is_empty());
        let v = PlacedValues::load(&placed.values).unwrap();
        assert_eq!(v.cases, ["everyday", "wind", "ice", "weather: glaze ice"]);
        assert_eq!(
            v.source("weather: glaze ice"),
            ohl_structure_loads::CaseSource::WeatherCase
        );
        // No load cases and no weather case chosen: no values, the file
        // says to tick weather cases.
        let mut inputs = LineInputs::of(&model);
        inputs.load_cases = None;
        assert_eq!(inputs.write_all(std::slice::from_ref(&placed)).len(), 1);
        let v = PlacedValues::load(&placed.values).unwrap();
        assert!(v.cases.is_empty());
        assert!(v.note.unwrap().contains(NO_CASES), "note");
        // Without load cases, the weather cases chosen alone.
        inputs.line.structure_loads.weather_cases = vec!["high wind".into()];
        assert!(inputs.write_all(std::slice::from_ref(&placed)).is_empty());
        let v = PlacedValues::load(&placed.values).unwrap();
        assert_eq!(v.cases, ["weather: high wind"]);
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

    /// Not found: the report says where it was looked for and how to build
    /// it. Another build: a warning with the command building both; the
    /// same build (or unknown builds of the same version): none.
    #[test]
    fn not_found_and_another_build_say_what_to_do() {
        let places = [PathBuf::from("/a/bin"), PathBuf::from("/b")];
        let text = not_found(&places);
        assert!(text.contains("    /a/bin\n    /b"), "{text}");
        assert!(text.contains(BUILD) && text.contains(PROGRAM_ENV), "{text}");
        let program = Path::new("/x/ols-structure");
        assert_eq!(version_warning(program, VERSION), None);
        let w = version_warning(program, "0.0.0 (build other)").unwrap();
        assert!(
            w.contains(BUILD_BOTH) && w.contains("0.0.0 (build other)"),
            "{w}"
        );
        assert!(version_warning(program, "").is_some());
        let number = env!("CARGO_PKG_VERSION");
        assert_eq!(
            version_warning(program, &format!("{number} (build unknown)")),
            None
        );
        assert!(VERSION.starts_with(number) && VERSION.contains("(build "));
    }

    /// A program that answers `--version` with another build, then stops
    /// at once with an error: started anyway with a warning, and its error
    /// output shown in an error window (a box of fixed height) until
    /// closed. A program that is not there: a report too.
    #[cfg(unix)]
    #[test]
    fn a_program_that_fails_shows_its_error_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ohl-design-fake-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("ols-structure");
        std::fs::write(
            &program,
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then echo 'ols-structure 0.0.0 (build other)'; exit 0; fi\n\
             echo \"error: cannot read $1: unknown field pole\" >&2\n\
             exit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.join("p.ohls");
        std::fs::write(&file, "id = \"p\"\n").unwrap();
        let mut apps = StructureApps {
            program: Some(program.clone()),
            ..StructureApps::default()
        };
        assert_eq!(version_of(&program).unwrap(), "0.0.0 (build other)");
        apps.launch(&file, "p", None, 0.0, None, None).unwrap();
        // The warning at once (started anyway).
        assert_eq!(apps.reports.len(), 1);
        assert!(!apps.reports[0].error);
        assert!(apps.reports[0].text.contains(BUILD_BOTH));
        // It stops: the error window, with its error output.
        let until = Instant::now() + Duration::from_secs(20);
        while apps.reports.len() < 2 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
            apps.poll();
        }
        let r = apps.reports.last().unwrap().clone();
        assert!(r.error, "{r:?}");
        assert_eq!(r.title, "OHL-Structure did not start");
        assert!(r.text.contains("cannot read"), "{}", r.text);
        assert!(r.text.contains("unknown field pole"), "{}", r.text);
        assert!(!apps.active());
        // Shown, and closed.
        let mut harness = Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .build_ui_state(
                |ui, apps: &mut StructureApps| report_window(ui.ctx(), apps),
                apps,
            );
        harness.run();
        harness.get_by_label_contains("unknown field pole");
        assert_eq!(
            harness.query_all_by_label_contains("rebuild both").count(),
            2
        );
        assert_eq!(harness.query_all_by_label("Close").count(), 2);
        harness.query_all_by_label("Close").next().unwrap().click();
        harness.run();
        assert_eq!(harness.state().reports.len(), 1);
        // Not there: why, in a report.
        let mut apps = StructureApps {
            program: Some(dir.join("gone")),
            ..StructureApps::default()
        };
        assert!(apps.launch(&file, "p", None, 0.0, None, None).is_err());
        assert!(
            apps.reports
                .iter()
                .any(|r| r.error && r.text.contains(BUILD))
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
