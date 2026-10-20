// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The user's plugin settings, shared by the applications (the line
//! application `ohl-design` and OHL-Structure): the plugin folders added and
//! the plugins turned off, in [`SETTINGS_FILE`] in the user's plugin
//! directory (beside the plugins installed there). A plugin added or turned
//! off in one application is so in the other.
//!
//! ```toml
//! # OpenOHL plugin settings (written by the applications)
//! folders = ["/Users/me/plugins-in-progress"]
//! disabled = ["geojson"]
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The settings file's name in the user's plugin directory.
pub const SETTINGS_FILE: &str = "settings.toml";

/// The plugin folders added and the plugins turned off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettings {
    /// Folders whose plugins are used where they are.
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    /// Plugins turned off, by name.
    #[serde(default)]
    pub disabled: BTreeSet<String>,
}

impl PluginSettings {
    /// The settings file of the user's plugin directory `user_dir`.
    pub fn path(user_dir: &Path) -> PathBuf {
        user_dir.join(SETTINGS_FILE)
    }

    /// Reads `path`: `Ok(None)` when there is no such file, `Err` when it
    /// does not read (what is wrong, with the file).
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        toml::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Writes `path` (its folder made if needed), through a temporary file
    /// renamed into place: another application reading it at that moment
    /// sees the old file or the new one, never half of it.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let at = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(at)?;
        }
        let body = toml::to_string(self).map_err(|e| e.to_string())?;
        let text = format!("# OpenOHL plugin settings (written by the applications)\n{body}");
        let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
        std::fs::write(&tmp, text).map_err(at)?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            at(e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written and read back; no file is no settings; a typo is an error
    /// naming the file.
    #[test]
    fn settings_round_trip() {
        let dir = std::env::temp_dir().join(format!("ohl-plugin-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = PluginSettings::path(&dir.join("plugins"));
        assert_eq!(PluginSettings::load(&path), Ok(None));
        let s = PluginSettings {
            folders: vec![PathBuf::from("/somewhere/plugins")],
            disabled: ["geojson".to_owned()].into(),
        };
        s.save(&path).unwrap();
        assert_eq!(PluginSettings::load(&path), Ok(Some(s)));
        std::fs::write(&path, "folder = []\n").unwrap();
        let e = PluginSettings::load(&path).unwrap_err();
        assert!(e.contains("settings.toml") && e.contains("folder"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
