// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Help › **User guide**, **Start from scratch (tutorial)** and **About**:
//! the manual on this computer (the book built by `mdbook build` in
//! `target/book` or `book`, else its Markdown sources in `docs/`), opened
//! with the system's program for the file — nothing is fetched from the
//! internet. Also where the files shipped with the program are found
//! (the starter template, the example).

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
    roots.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")));
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
    /// The glossary: one name per object, with drawings.
    Glossary,
}

impl Page {
    /// Every page, in the Help menu's order.
    pub const ALL: [Page; 3] = [Page::UserGuide, Page::Tutorial, Page::Glossary];

    /// Name in the Help menu.
    pub fn name(self) -> &'static str {
        match self {
            Page::UserGuide => "User guide",
            Page::Tutorial => "Start from scratch (tutorial)",
            Page::Glossary => "Glossary",
        }
    }

    /// What it opens, in the hover text.
    pub fn tip(self) -> &'static str {
        match self {
            Page::UserGuide => {
                "The manual of OHL-Design on this computer: every menu, page and tool"
            }
            Page::Tutorial => {
                "Your first line, step by step from an empty project made from the starter \
                 template (File › New project…, Starter): route, alignment, structures, \
                 strings, wires, jumpers"
            }
            Page::Glossary => {
                "One name per object: what each term means (structure, string, wire point, \
                 set, section, conductor path…), with drawings"
            }
        }
    }

    /// Why it is greyed out (no page found on this computer).
    pub fn missing(self) -> &'static str {
        match self {
            Page::UserGuide => {
                "The manual is not on this computer: build it with `mdbook build` in the \
                 OpenOHL folder (target/book), or read docs/ there"
            }
            Page::Tutorial => {
                "The tutorial is not on this computer: build the manual with `mdbook build` \
                 in the OpenOHL folder (target/book), or read docs/tutorial/index.md"
            }
            Page::Glossary => {
                "The glossary is not on this computer: build the manual with `mdbook build` \
                 in the OpenOHL folder (target/book), or read docs/user-guide/glossary.md"
            }
        }
    }

    /// Its pages in the book, the first found is opened (without
    /// extension: `.html` in the built book, `.md` in the sources).
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Page::UserGuide => &["user-guide/desktop-app", "introduction"],
            Page::Tutorial => &["tutorial/index", "tutorial/00-before-you-start"],
            Page::Glossary => &["user-guide/glossary"],
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

/// Opens the manual's page `page` (e.g. `user-guide/analyses`, without
/// extension) at section `anchor`: the built book's page, else its
/// Markdown source (the section's heading then to look for). What was
/// opened, or why not.
pub fn open_section(page: &str, anchor: &str) -> Result<String, String> {
    for b in ["target/book", "book"] {
        if let Some(p) = shipped(&format!("{b}/{page}.html")) {
            let url = format!("file://{}#{anchor}", p.display());
            open(Path::new(&url))?;
            return Ok(url);
        }
    }
    let p = shipped(&format!("docs/{page}.md"))
        .ok_or_else(|| format!("the manual is not on this computer ({page})"))?;
    open(&p)?;
    Ok(format!("{} (section {anchor})", p.display()))
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
            .on_disabled_hover_text(page.missing());
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

/// The About window's text: the program, its version and build, what it
/// is, its licence.
pub fn about_ui(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add(crate::icons::mark(ui.ctx(), 48.0));
        ui.heading(crate::APP_NAME);
    });
    ui.label(format!("Version {}", crate::structure_app::VERSION));
    ui.label(
        "The line application of OpenOHL: sag-tension, clearances and line layout in 3D, \
         plan and profile. Its structures open in OHL-Structure; the command line is ohl.",
    );
    ui.label(
        egui::RichText::new(
            "Free software under the GNU Affero General Public License v3.0 or later. Help › User guide, Start \
             from scratch and Glossary open the manual on this computer.",
        )
        .small()
        .weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The user guide, the glossary and the tutorial are found (the book
    /// or its sources).
    #[test]
    fn help_pages_resolve_to_files() {
        for page in Page::ALL {
            let f = page
                .file()
                .unwrap_or_else(|| panic!("{}: not found", page.name()));
            assert!(f.is_file(), "{}", f.display());
        }
        assert!(shipped("docs/tutorial/index.md").is_some());
        assert!(shipped("examples/starter/README.md").is_some());
    }
}
