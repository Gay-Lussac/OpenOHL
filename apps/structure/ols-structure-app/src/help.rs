// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Help › **User guide** and **Start from scratch (tutorial)**, as in
//! OHL-Design: the manual on this computer (the book built by `mdbook
//! build` in `target/book` or `book`, else its Markdown sources in
//! `docs/`), opened with the system's program for the file — nothing is
//! fetched from the internet.

use std::path::{Path, PathBuf};

use eframe::egui;

/// The first of the folders where `rel` (a path in the source tree, e.g.
/// `examples/starter`) exists: under the working folder, beside the
/// program or one of the folders above it, else in the source tree it was
/// built from.
pub fn shipped(rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().skip(1).map(Path::to_path_buf));
    }
    roots.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.."
    )));
    roots
        .into_iter()
        .map(|r| r.join(rel))
        .find(|p| p.exists())
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
}

/// A page of the manual the Help menu opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The user guide of this program.
    UserGuide,
    /// The tutorial, "Start from scratch — your first line".
    Tutorial,
}

impl Page {
    /// Every page, in the Help menu's order.
    pub const ALL: [Page; 2] = [Page::UserGuide, Page::Tutorial];

    /// Name in the Help menu.
    pub fn name(self) -> &'static str {
        match self {
            Page::UserGuide => "User guide",
            Page::Tutorial => "Start from scratch (tutorial)",
        }
    }

    /// What it opens, in the hover text.
    pub fn tip(self) -> &'static str {
        match self {
            Page::UserGuide => {
                "The manual of OHL-Structure on this computer: its panels, levels, loads and \
                 results"
            }
            Page::Tutorial => {
                "Your first line, step by step from an empty project made from the starter \
                 template (File › New project…, Starter): route, alignment, structures, \
                 strings, wires, jumpers"
            }
        }
    }

    /// Its pages in the book, the first found is opened (without
    /// extension: `.html` in the built book, `.md` in the sources).
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Page::UserGuide => &["user-guide/structure-app", "introduction"],
            Page::Tutorial => &["tutorial/index", "tutorial/00-before-you-start"],
        }
    }

    /// The file to open: the built book's page, else its Markdown source;
    /// `None` when neither is on this computer.
    pub fn file(self) -> Option<PathBuf> {
        let books = ["target/book", "book"];
        for page in self.candidates() {
            for b in books {
                if let Some(p) = shipped(&format!("{b}/{page}.html")) {
                    return Some(p);
                }
            }
        }
        self.candidates()
            .iter()
            .find_map(|page| shipped(&format!("docs/{page}.md")))
    }
}

/// Files opened (tests: nothing is started).
#[cfg(test)]
pub static OPENED: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Opens `path` with the system's program for it (a web page in the
/// browser, from this computer).
#[cfg(not(test))]
pub fn open(path: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("opening {}: {e}", path.display()))
}

/// Tests: the file is noted, nothing is started.
#[cfg(test)]
pub fn open(path: &Path) -> Result<(), String> {
    OPENED.lock().unwrap().push(path.to_path_buf());
    Ok(())
}

/// The Help menu's entries for the manual: one per [`Page`] (greyed out,
/// with where it looked, when the manual is not on this computer). What
/// went wrong goes to `status`.
pub fn manual_entries(ui: &mut egui::Ui, status: &mut Option<(String, bool)>) {
    for page in Page::ALL {
        let file = page.file();
        let r = ui
            .add_enabled(
                file.is_some(),
                egui::Button::new(format!("{}…", page.name())),
            )
            .on_hover_text(page.tip())
            .on_disabled_hover_text(
                "The manual is not on this computer: build it with `mdbook build` in the \
                 OpenOHL folder (target/book), or read docs/ there",
            );
        if r.clicked()
            && let Some(f) = file
        {
            *status = Some(match open(&f) {
                Ok(()) => (format!("{} opened: {}", page.name(), f.display()), false),
                Err(e) => (e, true),
            });
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The user guide is found (the book or its sources); the tutorial
    /// too once its chapters are in docs/tutorial.
    #[test]
    fn help_pages_resolve_to_files() {
        let guide = Page::UserGuide.file().expect("the user guide");
        assert!(guide.is_file(), "{}", guide.display());
        let tutorial = shipped("docs/tutorial");
        match Page::Tutorial.file() {
            Some(f) => assert!(f.is_file(), "{}", f.display()),
            None => assert!(tutorial.is_none(), "docs/tutorial has no first page"),
        }
    }
}
