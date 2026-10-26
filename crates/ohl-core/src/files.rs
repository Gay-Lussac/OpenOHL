// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Files on disk: format headers, the libraries shipped with OpenOHL and
//! library folders.

use std::path::{Path, PathBuf};

/// The extension of project files (`.ohl`).
pub const PROJECT_EXTENSION: &str = "ohl";

/// The ending of line files (`*.ohl.json`).
pub const LINE_FILE_SUFFIX: &str = ".ohl.json";

/// Whether `path` has `extension`.
pub fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == extension)
}

/// Whether `path` names a project file (`.ohl`).
pub fn is_project_file(path: &Path) -> bool {
    has_extension(path, PROJECT_EXTENSION)
}

/// The name of a line file without its ending (`.ohl.json`); `None` for
/// another name.
pub fn line_file_stem(name: &str) -> Option<&str> {
    name.strip_suffix(LINE_FILE_SUFFIX)
}

/// Checks a file's `format` and `version` header — the one policy of every
/// OpenOHL format: both optional; the format, given, must be `expected`;
/// any version up to
/// `supported` is read (older ones converted by the reader), a newer one
/// refused. `what` names the kind of file in the message ("rule",
/// "weather case", …).
pub fn check_header(
    format: Option<&str>,
    version: Option<u32>,
    expected: &str,
    supported: u32,
    what: &str,
) -> Result<(), String> {
    if let Some(f) = format.filter(|f| *f != expected) {
        return Err(format!("format \"{f}\" is not \"{expected}\""));
    }
    if version.is_some_and(|v| v > supported) {
        return Err(format!(
            "{what} format version {} newer than supported ({supported}): written by a newer OpenOHL",
            version.unwrap_or_default()
        ));
    }
    Ok(())
}

/// A folder of the libraries shipped with OpenOHL (`libraries/<kind>`):
/// in the current folder, else next to the program or above it (a build
/// in `target/release` of the source tree); the relative path if neither
/// exists.
pub fn shipped_library(kind: &str) -> PathBuf {
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

/// The files with `extension` in `dir`, sorted (none if it cannot be read).
pub fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| has_extension(p, extension))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The folder of `file` (`.` for a bare file name).
pub fn dir_of(file: &Path) -> PathBuf {
    file.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers() {
        assert!(check_header(None, None, "ohl-x", 1, "x").is_ok());
        assert!(check_header(Some("ohl-x"), Some(1), "ohl-x", 1, "x").is_ok());
        // The ids of before 2026-10-18 (`ols-…`) are no longer read.
        assert!(check_header(Some("ols-x"), Some(1), "ohl-x", 1, "x").is_err());
        assert_eq!(
            check_header(Some("ohl-y"), None, "ohl-x", 1, "x").unwrap_err(),
            "format \"ohl-y\" is not \"ohl-x\""
        );
        assert_eq!(
            check_header(None, Some(2), "ohl-x", 1, "x").unwrap_err(),
            "x format version 2 newer than supported (1): written by a newer OpenOHL"
        );
    }

    #[test]
    fn names() {
        assert!(has_extension(Path::new("a.ohlw"), "ohlw"));
        assert!(!has_extension(Path::new("a.olsw"), "ohlw"));
        assert!(!has_extension(Path::new("a.ohlwc"), "ohlw"));
        assert!(!has_extension(Path::new("a.csv"), "csv.x"));
        assert!(is_project_file(Path::new("d/p.ohl")) && !is_project_file(Path::new("p.ols")));
        assert!(!is_project_file(Path::new("p.ohl.json")));
        assert_eq!(line_file_stem("a.ohl.json"), Some("a"));
        assert_eq!(line_file_stem("a.ols.json"), None);
        assert_eq!(line_file_stem("a.json"), None);
    }

    #[test]
    fn folders() {
        assert_eq!(dir_of(Path::new("a.ohl")), Path::new("."));
        assert_eq!(dir_of(Path::new("d/a.ohl")), Path::new("d"));
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let found = files_with_extension(&here.join("src"), "rs");
        assert!(found.windows(2).all(|w| w[0] < w[1]));
        assert!(found.iter().any(|p| p.ends_with("files.rs")));
        assert!(files_with_extension(&here.join("no-such-dir"), "rs").is_empty());
    }
}
