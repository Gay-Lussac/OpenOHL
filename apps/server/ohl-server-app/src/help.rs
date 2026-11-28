// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The user guide's pages, opened from the app (the built book next to
//! the program or in the repository, else the Markdown source), and the
//! browser for a sign-in.

use std::path::{Path, PathBuf};

/// Pages and browser addresses opened.
pub static OPENED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Tests set this: nothing is opened, only noted in [`OPENED`].
pub static DRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn shipped(rel: &str) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().skip(1).take(6).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    roots.into_iter().map(|r| r.join(rel)).find(|p| p.is_file())
}

fn open_with_system(target: &str) -> Result<(), String> {
    OPENED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(target.to_owned());
    if DRY.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("opening {target}: {e}"))
}

/// Opens web address `url` in the system's browser (the organisation's
/// sign-in page).
pub fn open_url(url: &str) -> Result<(), String> {
    open_with_system(url)
}

/// Opens the manual's page `page` (without extension) at `anchor`.
pub fn open_section(page: &str, anchor: &str) -> Result<(), String> {
    for b in ["target/book", "book"] {
        if let Some(p) = shipped(&format!("{b}/{page}.html")) {
            return open_with_system(&format!("file://{}#{anchor}", p.display()));
        }
    }
    let p = shipped(&format!("docs/{page}.md"))
        .ok_or_else(|| format!("the manual is not on this computer ({page})"))?;
    open_with_system(&p.display().to_string())
}
