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
//!
//! # What the user allowed, per plugin version (security review
//! # 2026-10-26): a Python plugin unsandboxed, a WebAssembly plugin more
//! # time or memory a call than the defaults.
//! [allowed."my-script@0.1.0"]
//! unsandboxed = true
//! [allowed."esri-grid@0.1.0"]
//! memory-mib = 1024
//! ```
//!
//! `allowed` is written only when something is allowed (an older
//! application reads the file as before until then).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    /// What the user allowed, by plugin version (`name@version`,
    /// [`consent_key`]): asked once in the plugin manager, asked again for
    /// another version.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub allowed: BTreeMap<String, Consent>,
}

/// What the user allowed a plugin version (security review 2026-10-26):
/// to run unsandboxed (a Python plugin), more time or memory a call than
/// the defaults (a WebAssembly plugin, up to these).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Consent {
    /// Runs with the user's rights (a Python plugin).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unsandboxed: bool,
    /// Longest call, seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<u64>,
    /// Largest memory, MiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u64>,
}

impl Consent {
    /// In words: what is (to be) allowed.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.unsandboxed {
            parts.push(
                "runs Python with your rights — not sandboxed: it may read, change or send                  anything you may"
                    .to_owned(),
            );
        }
        if let Some(s) = self.seconds {
            parts.push(format!("up to {s} s a call (default 5 s)"));
        }
        if let Some(m) = self.memory_mib {
            parts.push(format!("up to {m} MiB of memory (default 256 MiB)"));
        }
        parts.join("; ")
    }
}

/// The key of a plugin version in [`PluginSettings::allowed`].
pub fn consent_key(name: &str, version: &str) -> String {
    format!("{name}@{version}")
}

/// What the user allowed, for the plugins this program loads (set by the
/// applications from their settings).
static CONSENTS: Mutex<BTreeMap<String, Consent>> = Mutex::new(BTreeMap::new());

/// Sets what the user allowed (from [`PluginSettings::allowed`]).
pub fn set_consents(allowed: &BTreeMap<String, Consent>) {
    *CONSENTS.lock().unwrap_or_else(|e| e.into_inner()) = allowed.clone();
}

/// What the user allowed plugin `name` of version `version`.
pub fn consent_for(name: &str, version: &str) -> Option<Consent> {
    CONSENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&consent_key(name, version))
        .copied()
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
        let mut s = PluginSettings {
            folders: vec![PathBuf::from("/somewhere/plugins")],
            disabled: ["geojson".to_owned()].into(),
            ..PluginSettings::default()
        };
        s.save(&path).unwrap();
        // Nothing allowed: the file as before (older applications read it).
        assert!(!std::fs::read_to_string(&path).unwrap().contains("allowed"));
        assert_eq!(PluginSettings::load(&path), Ok(Some(s.clone())));
        s.allowed.insert(
            consent_key("grid", "0.1.0"),
            Consent {
                memory_mib: Some(1024),
                ..Consent::default()
            },
        );
        s.allowed.insert(
            consent_key("script", "1.0.0"),
            Consent {
                unsandboxed: true,
                ..Consent::default()
            },
        );
        s.save(&path).unwrap();
        assert_eq!(PluginSettings::load(&path), Ok(Some(s.clone())));
        std::fs::write(&path, "folder = []\n").unwrap();
        let e = PluginSettings::load(&path).unwrap_err();
        assert!(e.contains("settings.toml") && e.contains("folder"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
