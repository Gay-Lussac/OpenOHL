// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Files on disk: format headers, the libraries shipped with OpenOHL and
//! library folders.

use std::path::{Path, PathBuf};

/// The extension of project files (`.ohl`; [`LEGACY_PROJECT_EXTENSION`]
/// before 2026-10-18, still opened).
pub const PROJECT_EXTENSION: &str = "ohl";

/// The extension project files had before 2026-10-18 (still opened).
pub const LEGACY_PROJECT_EXTENSION: &str = "ols";

/// The ending of line files (`*.ohl.json`; [`LEGACY_LINE_FILE_SUFFIX`]
/// before 2026-10-18, still read).
pub const LINE_FILE_SUFFIX: &str = ".ohl.json";

/// The ending line files had before 2026-10-18 (still read).
pub const LEGACY_LINE_FILE_SUFFIX: &str = ".ols.json";

/// The name a file extension or format id had before 2026-10-18, when
/// file extensions and format ids changed from `ols` to `ohl`
/// (`ohlw` → `olsw`, `ohl-structure` → `ols-structure`); `None` for one
/// that never had another name. Readers accept both.
pub fn legacy_name(name: &str) -> Option<String> {
    name.strip_prefix("ohl").map(|rest| format!("ols{rest}"))
}

/// Whether `found` is the format id `expected` or its name before
/// 2026-10-18 ([`legacy_name`]).
pub fn is_format(found: &str, expected: &str) -> bool {
    found == expected || legacy_name(expected).is_some_and(|l| found == l)
}

/// Whether `path` has `extension` or its name before 2026-10-18
/// ([`legacy_name`]: `.olsw` for `ohlw`).
pub fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| is_format(e, extension))
}

/// The extensions a file dialog opening `extension` files offers: it and,
/// for files of before 2026-10-18, its old name ([`legacy_name`]).
pub fn open_extensions(extension: &str) -> Vec<String> {
    std::iter::once(extension.to_owned())
        .chain(legacy_name(extension))
        .collect()
}

/// Whether `path` exists, or the file of its name with the extension's
/// name before 2026-10-18 (`a.olsw` for `a.ohlw`).
pub fn exists_by_either_name(path: &Path) -> bool {
    path.exists()
        || path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(legacy_name)
            .is_some_and(|old| path.with_extension(old).exists())
}

/// Whether `path` names a project file (`.ohl`, or `.ols` of before
/// 2026-10-18).
pub fn is_project_file(path: &Path) -> bool {
    has_extension(path, PROJECT_EXTENSION)
}

/// The name of a line file without its ending (`.ohl.json`, or
/// `.ols.json` of before 2026-10-18); `None` for another name.
pub fn line_file_stem(name: &str) -> Option<&str> {
    name.strip_suffix(LINE_FILE_SUFFIX)
        .or_else(|| name.strip_suffix(LEGACY_LINE_FILE_SUFFIX))
}

/// Checks a file's `format` and `version` header — the one policy of every
/// OpenOHL format: both optional; the format, given, must be `expected`
/// (or its name before 2026-10-18, [`legacy_name`]); any version up to
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
    if let Some(f) = format.filter(|f| !is_format(f, expected)) {
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

/// The files with `extension` in `dir`, sorted (none if it cannot be read);
/// with the extension's name before 2026-10-18 too ([`has_extension`]),
/// except where a file of the same name has the new one.
pub fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| has_extension(p, extension))
                .collect()
        })
        .unwrap_or_default();
    let renamed: Vec<PathBuf> = out
        .iter()
        .filter(|p| p.extension().is_some_and(|x| x != extension))
        .filter(|p| out.contains(&p.with_extension(extension)))
        .cloned()
        .collect();
    out.retain(|p| !renamed.contains(p));
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
        // The id of before 2026-10-18 is read too.
        assert!(check_header(Some("ols-x"), Some(1), "ohl-x", 1, "x").is_ok());
        assert_eq!(
            check_header(Some("ohl-y"), None, "ohl-x", 1, "x").unwrap_err(),
            "format \"ohl-y\" is not \"ohl-x\""
        );
        assert!(check_header(Some("ols-y"), None, "ohl-x", 1, "x").is_err());
        assert_eq!(
            check_header(None, Some(2), "ohl-x", 1, "x").unwrap_err(),
            "x format version 2 newer than supported (1): written by a newer OpenOHL"
        );
    }

    #[test]
    fn old_names() {
        assert_eq!(legacy_name("ohlw").as_deref(), Some("olsw"));
        assert_eq!(
            legacy_name("ohl-structure").as_deref(),
            Some("ols-structure")
        );
        assert_eq!(legacy_name("csv"), None);
        assert_eq!(open_extensions("ohlw"), ["ohlw", "olsw"]);
        assert_eq!(open_extensions("csv"), ["csv"]);
        assert!(has_extension(Path::new("a.olsw"), "ohlw"));
        assert!(has_extension(Path::new("a.ohlw"), "ohlw"));
        assert!(!has_extension(Path::new("a.olswc"), "ohlw"));
        assert!(!has_extension(Path::new("a.csv"), "csv.x"));
        assert!(is_project_file(Path::new("d/p.ohl")) && is_project_file(Path::new("p.ols")));
        assert!(!is_project_file(Path::new("p.ols.json")));
        assert_eq!(line_file_stem("a.ohl.json"), Some("a"));
        assert_eq!(line_file_stem("a.ols.json"), Some("a"));
        assert_eq!(line_file_stem("a.json"), None);
        // Both extensions listed; a renamed copy hides its old file.
        let dir = std::env::temp_dir().join(format!("ohl-core-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["a.olsw", "b.ohlw", "b.olsw", "c.ohlw", "d.olswc"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let names: Vec<String> = files_with_extension(&dir, "ohlw")
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.olsw", "b.ohlw", "c.ohlw"]);
        std::fs::remove_dir_all(&dir).unwrap();
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
