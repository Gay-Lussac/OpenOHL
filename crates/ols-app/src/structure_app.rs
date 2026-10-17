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

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant, SystemTime};

/// The structure application's program name.
pub const PROGRAM: &str = "ols-structure";

/// How often the files are looked at.
const POLL: Duration = Duration::from_millis(500);

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
}

/// The structure applications started, and their files.
#[derive(Default)]
pub struct StructureApps {
    watched: Vec<Watched>,
    last: Option<Instant>,
}

impl StructureApps {
    /// Starts the structure application on `path` (structure `id`), with
    /// the insulator assemblies of `insulators` and the angle `ahead`; the
    /// file is watched from now on. One already running on the file is not
    /// started again.
    pub fn launch(
        &mut self,
        path: &Path,
        id: &str,
        insulators: Option<&Path>,
        ahead: f64,
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
        let program = find()?;
        let mut command = std::process::Command::new(&program);
        command.arg(path).arg("--ahead").arg(ahead.to_string());
        if let Some(d) = insulators {
            command.arg("--insulators").arg(d);
        }
        let child = command
            .spawn()
            .map_err(|e| format!("{}: {e}", program.display()))?;
        self.watch(path, id, Some(child));
        Ok(())
    }

    /// Watches `path` (structure `id`), edited by `child` (none: by
    /// someone else).
    pub fn watch(&mut self, path: &Path, id: &str, child: Option<Child>) {
        let (modified, text) = read(path);
        self.watched.retain(|w| w.path != path);
        self.watched.push(Watched {
            path: path.to_path_buf(),
            id: id.to_owned(),
            child,
            modified,
            text,
        });
    }

    /// Whether a file is watched (the caller asks for a frame now and
    /// then).
    pub fn active(&self) -> bool {
        !self.watched.is_empty()
    }

    /// The files saved since last looked at (path, structure id), at most
    /// every half second; files whose application has ended are looked at
    /// once more and dropped.
    pub fn poll(&mut self) -> Vec<(PathBuf, String)> {
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
                    changed.push((w.path.clone(), w.id.clone()));
                }
            }
        }
        self.watched.retain_mut(|w| match &mut w.child {
            Some(c) => c.try_wait().ok().flatten().is_none(),
            None => true,
        });
        changed
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
            r.structure_apps.watch(&path, "type-02", None);
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
}
