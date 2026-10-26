// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Plugin manifests, compatibility checks and the WebAssembly component host.
//!
//! Every plugin ships an `ohl-plugin.toml` manifest (ADR-0003; `ols-plugin.toml`,
//! its name before 2026-10-18, is read too):
//!
//! ```toml
//! [plugin]
//! name = "hello-wasm"
//! version = "0.1.0"
//! description = "Example plugin"
//! sdk = "0.3"                 # plugin interface version it targets
//! tier = "wasm"               # "wasm" or "python"
//! entry = "hello_wasm.wasm"   # relative to the manifest
//! extension-points = []
//!
//! [permissions]               # everything denied unless requested
//! network = false
//! filesystem = []
//! ```
//!
//! WebAssembly plugins implement the `ohl:plugin` WIT world in `wit/` and run
//! sandboxed: no file system, network, environment or standard I/O access.
//!
//! Every call into a plugin has exit clauses ([`Limits`]): a time limit (an
//! endless loop is interrupted and reported as [`PluginError::Timeout`]) and
//! a memory cap.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Deserialize;

pub mod settings;
pub use settings::{
    Consent, PluginSettings, SETTINGS_FILE, STORAGE_ID, USER_DIR, consent_for, set_consents,
    user_dir,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod bindings {
    wasmtime::component::bindgen!({
        world: "plugin",
        path: "wit",
    });
}

mod io_bindings {
    wasmtime::component::bindgen!({
        world: "io-plugin",
        path: "wit",
    });
}

mod terrain_bindings {
    wasmtime::component::bindgen!({
        world: "terrain-plugin",
        path: "wit",
    });
}

pub use terrain_bindings::ohl::plugin::terrain_types::Surface;

mod project_bindings {
    wasmtime::component::bindgen!({
        world: "project-plugin",
        path: "wit",
    });
}

pub use project_bindings::ohl::plugin::project_types::{LibraryFile, Project};

mod code_bindings {
    wasmtime::component::bindgen!({
        world: "code-plugin",
        path: "wit",
    });
}

pub use code_bindings::ohl::plugin::code_types::{
    ClearanceMeasure, ClearancePreset, CodeInfo, Generated,
};

mod code_wind_bindings {
    wasmtime::component::bindgen!({
        world: "code-wind-plugin",
        path: "wit",
    });
}

/// Export name of the optional `structure-wind` interface of design-code
/// plugins.
pub const STRUCTURE_WIND_EXPORT: &str = "ohl:plugin/structure-wind@0.1.0";

mod rule_presets_bindings {
    wasmtime::component::bindgen!({
        world: "rule-presets-export",
        path: "wit",
    });
}

pub use rule_presets_bindings::exports::ohl::plugin::rule_presets::RulePreset;

/// Export name of the optional `rule-presets` interface of design-code
/// plugins.
pub const RULE_PRESETS_EXPORT: &str = "ohl:plugin/rule-presets@0.1.0";

mod sheet_templates_bindings {
    wasmtime::component::bindgen!({
        world: "sheet-templates-export",
        path: "wit",
    });
}

pub use sheet_templates_bindings::exports::ohl::plugin::sheet_templates::{
    SheetImage, SheetTemplate,
};

/// Export name of the optional `sheet-templates` interface of any plugin.
pub const SHEET_TEMPLATES_EXPORT: &str = "ohl:plugin/sheet-templates@0.1.0";

/// Extension point of plugins giving sheet templates (the
/// `sheet-templates` export, or `[[sheet-template]]` files in the
/// manifest).
pub const SHEET_TEMPLATES: &str = "sheet-templates";

mod check_bindings {
    wasmtime::component::bindgen!({
        world: "check-plugin",
        path: "wit",
    });
}

pub use check_bindings::exports::ohl::plugin::check::{
    CheckInfo, Choice, ConductorPair, Evaluated, ValueTable,
};

/// Extension point of check plugins (`check-plugin` world).
pub const CHECK: &str = "check";

/// Extension point of design-code plugins (`code-plugin` world).
pub const DESIGN_CODE: &str = "design-code";

pub use bindings::ohl::plugin::types::PluginInfo;
pub use io_bindings::ohl::plugin::io_types::{Direction, Format};

/// Version (major, minor) of the plugin interface implemented by this host.
pub const SDK_VERSION: (u64, u64) = (0, 3);

/// The plugin interface of before 2026-10-18 (`ols:plugin`, SDK 0.2): its
/// plugins do not load here; rebuilt for [`SDK_VERSION`] (the `ohl:plugin`
/// interface, nothing else changed) they do.
pub const LEGACY_SDK: &str = "0.2";

/// File name of plugin manifests.
pub const MANIFEST_FILE: &str = "ohl-plugin.toml";

/// File name of plugin manifests before 2026-10-18 (still read).
pub const LEGACY_MANIFEST_FILE: &str = "ols-plugin.toml";

/// The manifest of the plugin in `dir`: [`MANIFEST_FILE`], else
/// [`LEGACY_MANIFEST_FILE`] when only that one is there.
pub fn manifest_path(dir: &Path) -> PathBuf {
    let new = dir.join(MANIFEST_FILE);
    let old = dir.join(LEGACY_MANIFEST_FILE);
    if !new.is_file() && old.is_file() {
        old
    } else {
        new
    }
}

/// Errors raised while loading or running plugins.
#[derive(Debug)]
pub enum PluginError {
    /// The manifest could not be read or parsed.
    Manifest {
        /// Manifest path.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// The plugin targets an interface version this host does not support.
    Incompatible {
        /// Plugin name.
        plugin: String,
        /// Version the plugin targets.
        requested: String,
    },
    /// The plugin requests permissions that are not granted.
    PermissionDenied {
        /// Plugin name.
        plugin: String,
        /// Description of the requested permission.
        permission: String,
    },
    /// The plugin tier cannot be loaded by this host.
    UnsupportedTier(String),
    /// A plugin call exceeded its time limit and was interrupted.
    Timeout {
        /// Plugin name.
        plugin: String,
        /// Function that was running.
        function: &'static str,
        /// The limit that was exceeded.
        limit: Duration,
    },
    /// Error from the WebAssembly runtime (compilation, instantiation, a trap
    /// or exceeding the memory cap).
    Runtime(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest { path, message } => write!(f, "{}: {message}", path.display()),
            Self::Incompatible { plugin, requested } => write!(
                f,
                "plugin {plugin} targets plugin interface {requested}, host supports {}.{}{}",
                SDK_VERSION.0,
                SDK_VERSION.1,
                if requested == LEGACY_SDK {
                    ": rebuild it for the ohl:plugin interface (sdk = \"0.3\" in ohl-plugin.toml)"
                } else {
                    ""
                }
            ),
            Self::PermissionDenied { plugin, permission } => {
                write!(
                    f,
                    "plugin {plugin} requests a permission that is not granted: {permission}"
                )
            }
            Self::UnsupportedTier(tier) => {
                write!(f, "plugin tier \"{tier}\" cannot be loaded by this host")
            }
            Self::Timeout {
                plugin,
                function,
                limit,
            } => write!(
                f,
                "plugin {plugin}: {function} did not finish within {limit:?} and was interrupted \
                 (endless loop or too much work for one call)"
            ),
            Self::Runtime(message) => write!(f, "plugin runtime error: {message}"),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<wasmtime::Error> for PluginError {
    fn from(error: wasmtime::Error) -> Self {
        Self::Runtime(format!("{error:#}"))
    }
}

/// How a plugin is implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// WebAssembly component, sandboxed.
    Wasm,
    /// Python package or script, run through the Python bindings.
    Python,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Wasm => "wasm",
            Self::Python => "python",
        })
    }
}

/// `[plugin]` section of a manifest.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct PluginSection {
    /// Unique plugin name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// One-line description.
    #[serde(default)]
    pub description: String,
    /// Plugin interface version targeted, `"major.minor"`.
    pub sdk: String,
    /// Implementation tier.
    pub tier: Tier,
    /// Entry file, relative to the manifest.
    pub entry: PathBuf,
    /// Extension points the plugin contributes to.
    #[serde(default)]
    pub extension_points: Vec<String>,
}

/// `[permissions]` section of a manifest. Everything is denied by default.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Network access.
    #[serde(default)]
    pub network: bool,
    /// Directories the plugin wants to read or write.
    #[serde(default)]
    pub filesystem: Vec<String>,
}

/// `[limits]` section of a manifest: more time or memory than the host's
/// defaults for one call (a plugin reading large files), granted up to
/// [`Limits::MAX_TIMEOUT`] and [`Limits::MAX_MEMORY_BYTES`].
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct RequestedLimits {
    /// Longest call, seconds.
    #[serde(default)]
    pub seconds: Option<u64>,
    /// Largest memory, MiB.
    #[serde(default)]
    pub memory_mib: Option<u64>,
}

/// A `[[column]]` of a manifest: a column the plugin adds to one of the
/// app's tables. Its values are kept with the objects under
/// `plugin.name` ([`Manifest::field_key`]) and saved with the project; a
/// project opened without the plugin still shows them (unused).
///
/// ```toml
/// [[column]]
/// table = "circuits"        # structures, sections, circuits, systems,
///                           # jumpers, alignments or routes
/// name = "insulation_level" # unique in the plugin: letters, digits, _ -
/// title = "insulation level"
/// help = "LeV Table 2.4.1: the row of the lightning impulse level"
/// values = ["low", "high"]  # a choice; none: typed
/// default = "high"          # shown where not set
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableColumn {
    /// The table it is added to.
    pub table: String,
    /// Its name in the plugin.
    pub name: String,
    /// Its header (default: its name).
    #[serde(default)]
    pub title: String,
    /// What it holds, in words.
    #[serde(default)]
    pub help: String,
    /// The values it is chosen from; none: typed.
    #[serde(default)]
    pub values: Vec<String>,
    /// The value shown where none is set.
    #[serde(default)]
    pub default: Option<String>,
}

/// A `[[sheet-template]]` of a manifest: a sheet template file (`.ohlsheet`)
/// the plugin ships, no code needed; its images next to it.
///
/// ```toml
/// [[sheet-template]]
/// file = "templates/a1.ohlsheet"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestTemplate {
    /// The file, inside the plugin's folder.
    pub file: PathBuf,
}

/// A sheet template a plugin gives ([`WasmHost::sheet_templates`]): from
/// its `sheet-templates` export, or a file its manifest names.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplatePreset {
    /// Its name (a file's: the file name without its extension).
    pub name: String,
    /// What it is for.
    pub description: String,
    /// The template's text (.ohlsheet).
    pub template: String,
    /// Its images: name, media type, bytes (a file's: read next to it).
    pub images: Vec<(String, String, Vec<u8>)>,
    /// The folder its images are read from (a file's), else none.
    pub dir: Option<PathBuf>,
}

/// The tables a plugin may add columns to.
pub const COLUMN_TABLES: [&str; 7] = [
    "structures",
    "sections",
    "circuits",
    "systems",
    "jumpers",
    "alignments",
    "routes",
];

/// A parsed `ohl-plugin.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Plugin identity and entry point.
    pub plugin: PluginSection,
    /// Requested permissions.
    #[serde(default)]
    pub permissions: Permissions,
    /// Requested limits (more than the defaults).
    #[serde(default)]
    pub limits: RequestedLimits,
    /// `[[column]]`s the plugin adds to the app's tables.
    #[serde(default, rename = "column")]
    pub columns: Vec<TableColumn>,
    /// `[[sheet-template]]` files the plugin ships.
    #[serde(default, rename = "sheet-template")]
    pub sheet_templates: Vec<ManifestTemplate>,
    /// `[[plan-set]]` files the plugin ships (2026-10-24): the drawings to
    /// make in one go with its sheet templates (`.ohlset`).
    #[serde(default, rename = "plan-set")]
    pub plan_sets: Vec<ManifestTemplate>,
    /// Directory containing the manifest.
    #[serde(skip)]
    pub dir: PathBuf,
}

impl Manifest {
    /// Parses a manifest from text. `dir` is the directory it belongs to.
    pub fn parse(text: &str, dir: &Path) -> Result<Self, PluginError> {
        let mut manifest: Self = toml::from_str(text).map_err(|e| PluginError::Manifest {
            path: manifest_path(dir),
            message: e.to_string(),
        })?;
        manifest.dir = dir.to_owned();
        // The entry stays inside the plugin's folder (it is copied there on
        // install).
        let inside = manifest
            .plugin
            .entry
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside || manifest.plugin.entry.as_os_str().is_empty() {
            return Err(PluginError::Manifest {
                path: manifest_path(dir),
                message: format!(
                    "entry {}: a path inside the plugin's folder",
                    manifest.plugin.entry.display()
                ),
            });
        }
        let bad = |message: String| PluginError::Manifest {
            path: manifest_path(dir),
            message,
        };
        for (what, list) in [
            ("sheet-template", &manifest.sheet_templates),
            ("plan-set", &manifest.plan_sets),
        ] {
            for t in list {
                let inside = t
                    .file
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)));
                if !inside || t.file.as_os_str().is_empty() {
                    return Err(bad(format!(
                        "{what} {}: a file inside the plugin's folder",
                        t.file.display()
                    )));
                }
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for c in &manifest.columns {
            if !COLUMN_TABLES.contains(&c.table.as_str()) {
                return Err(bad(format!(
                    "column {}: table \"{}\" is not one of {}",
                    c.name,
                    c.table,
                    COLUMN_TABLES.join(", ")
                )));
            }
            let ok = |s: &str| {
                !s.is_empty()
                    && s.chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            };
            if !ok(&c.name) {
                return Err(bad(format!(
                    "column \"{}\": a name of letters, digits, _ and -",
                    c.name
                )));
            }
            if !names.insert((c.table.clone(), c.name.clone())) {
                return Err(bad(format!("column {} twice in {}", c.name, c.table)));
            }
            if let Some(d) = &c.default
                && !c.values.is_empty()
                && !c.values.contains(d)
            {
                return Err(bad(format!(
                    "column {}: default \"{d}\" is not one of its values",
                    c.name
                )));
            }
        }
        Ok(manifest)
    }

    /// The key a column's values are kept under on the objects:
    /// `plugin.column`.
    pub fn field_key(&self, column: &TableColumn) -> String {
        format!("{}.{}", self.plugin.name, column.name)
    }

    /// Loads `ohl-plugin.toml` (or `ols-plugin.toml` of before 2026-10-18)
    /// from a plugin directory.
    pub fn load(dir: &Path) -> Result<Self, PluginError> {
        let path = manifest_path(dir);
        let text = std::fs::read_to_string(&path).map_err(|e| PluginError::Manifest {
            path: path.clone(),
            message: e.to_string(),
        })?;
        Self::parse(&text, dir)
    }

    /// Absolute path of the entry file.
    pub fn entry_path(&self) -> PathBuf {
        self.dir.join(&self.plugin.entry)
    }

    /// Checks that the plugin targets an interface version this host supports.
    ///
    /// Before 1.0, major and minor must match exactly. From 1.0, the major
    /// must match and the plugin's minor must not exceed the host's.
    pub fn check_compatible(&self) -> Result<(), PluginError> {
        let incompatible = || PluginError::Incompatible {
            plugin: self.plugin.name.clone(),
            requested: self.plugin.sdk.clone(),
        };
        let (major, minor) = self
            .plugin
            .sdk
            .split_once('.')
            .and_then(|(a, b)| Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)))
            .ok_or_else(incompatible)?;
        let (host_major, host_minor) = SDK_VERSION;
        let compatible = if host_major == 0 {
            major == 0 && minor == host_minor
        } else {
            major == host_major && minor <= host_minor
        };
        if compatible {
            Ok(())
        } else {
            Err(incompatible())
        }
    }

    /// Checks that every requested permission can be granted.
    ///
    /// No permissions can be granted yet; granting will be tied to user and
    /// server roles (DESIGN_GOALS §4).
    pub fn check_permissions(&self) -> Result<(), PluginError> {
        let denied = |permission: String| PluginError::PermissionDenied {
            plugin: self.plugin.name.clone(),
            permission,
        };
        if self.permissions.network {
            return Err(denied("network".into()));
        }
        if let Some(dir) = self.permissions.filesystem.first() {
            return Err(denied(format!("filesystem: {dir}")));
        }
        Ok(())
    }

    /// What the user must allow before this plugin is used as it asks
    /// (security review 2026-10-26, L1 and M6): a Python plugin runs
    /// unsandboxed, with the user's rights; a WebAssembly plugin asking
    /// more time or memory a call than the defaults ([`Limits`]).
    /// Nothing to allow: the default (all off).
    pub fn asks(&self) -> Consent {
        let d = Limits::default();
        let python = self.plugin.tier == Tier::Python;
        Consent {
            unsandboxed: python,
            seconds: self
                .limits
                .seconds
                .filter(|s| !python && Duration::from_secs(*s) > d.call_timeout),
            memory_mib: self
                .limits
                .memory_mib
                .filter(|m| !python && m.saturating_mul(1 << 20) > d.max_memory_bytes as u64),
        }
    }

    /// What is still to allow ([`Manifest::asks`] beyond `given`), if
    /// anything.
    pub fn to_allow(&self, given: Option<&Consent>) -> Option<Consent> {
        let asked = self.asks();
        let given = given.copied().unwrap_or_default();
        let open = Consent {
            unsandboxed: asked.unsandboxed && !given.unsandboxed,
            seconds: asked
                .seconds
                .filter(|s| given.seconds.is_none_or(|g| g < *s)),
            memory_mib: asked
                .memory_mib
                .filter(|m| given.memory_mib.is_none_or(|g| g < *m)),
        };
        (open != Consent::default()).then_some(asked)
    }

    /// The key of this plugin version in the consents (`name@version`).
    pub fn consent_key(&self) -> String {
        settings::consent_key(&self.plugin.name, &self.plugin.version)
    }
}

/// A plugin directory found by [`discover`]: its manifest, or why it does
/// not read.
#[derive(Debug, Clone)]
pub struct Found {
    /// The plugin's directory (holding `ohl-plugin.toml`).
    pub dir: PathBuf,
    /// The manifest, or the error reading it.
    pub manifest: Result<Manifest, String>,
}

impl Found {
    /// Why the plugin cannot be used here (unreadable manifest, other
    /// interface version, permissions not granted, entry file missing);
    /// `None` when it can.
    pub fn problem(&self) -> Option<String> {
        let m = match &self.manifest {
            Err(e) => return Some(e.clone()),
            Ok(m) => m,
        };
        if let Err(e) = m.check_compatible() {
            return Some(e.to_string());
        }
        // A Python plugin runs with the user's rights: its permissions
        // section restricts nothing, so it is not "checked" (it is allowed
        // or not: [`Manifest::asks`]).
        if m.plugin.tier != Tier::Python
            && let Err(e) = m.check_permissions()
        {
            return Some(e.to_string());
        }
        if !m.entry_path().is_file() {
            return Some(format!(
                "entry file {} not found (not built?)",
                m.entry_path().display()
            ));
        }
        None
    }

    /// Whether its manifest lists the extension point `point` (e.g.
    /// [`DESIGN_CODE`]).
    pub fn provides(&self, point: &str) -> bool {
        self.manifest
            .as_ref()
            .is_ok_and(|m| m.plugin.extension_points.iter().any(|e| e == point))
    }

    /// The plugin's name (the directory's when the manifest does not read).
    pub fn name(&self) -> String {
        match &self.manifest {
            Ok(m) => m.plugin.name.clone(),
            Err(_) => self
                .dir
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        }
    }
}

/// The plugins in `root`: `root` itself when it holds a manifest, else each
/// directory directly in it that does (sorted by directory name). A missing
/// `root` holds none.
pub fn discover(root: &Path) -> Vec<Found> {
    let found = |dir: &Path| Found {
        dir: dir.to_owned(),
        manifest: Manifest::load(dir).map_err(|e| e.to_string()),
    };
    if manifest_path(root).is_file() {
        return vec![found(root)];
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| manifest_path(p).is_file())
        .collect();
    dirs.sort();
    dirs.iter().map(|d| found(d)).collect()
}

/// Installs the plugin in directory `from` into `root` (the user's plugin
/// directory), as `root/<name>`: the manifest and its entry file for a
/// WebAssembly plugin (not its build tree); the whole directory, without
/// `target` and hidden entries, for a Python plugin. Refused when the
/// plugin cannot be used ([`Found::problem`]) or, unless `replace`, when
/// it is installed already. Returns the installed directory.
pub fn install(from: &Path, root: &Path, replace: bool) -> Result<PathBuf, PluginError> {
    let io = |e: std::io::Error| PluginError::Runtime(e.to_string());
    let found = Found {
        dir: from.to_owned(),
        manifest: Manifest::load(from).map_err(|e| e.to_string()),
    };
    if let Some(problem) = found.problem() {
        return Err(PluginError::Runtime(problem));
    }
    let manifest = found.manifest.expect("no problem: it reads");
    let name = &manifest.plugin.name;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        || name.starts_with('.')
    {
        return Err(PluginError::Runtime(format!(
            "plugin name \"{name}\": use letters, digits, - _ ."
        )));
    }
    let to = root.join(name);
    if std::fs::canonicalize(&to).ok() == std::fs::canonicalize(from).ok() && to.exists() {
        return Err(PluginError::Runtime(format!(
            "{name} is installed from there"
        )));
    }
    if to.exists() {
        if !replace {
            return Err(PluginError::Runtime(format!(
                "{name} is installed already ({})",
                to.display()
            )));
        }
        std::fs::remove_dir_all(&to).map_err(io)?;
    }
    std::fs::create_dir_all(&to).map_err(io)?;
    match manifest.plugin.tier {
        Tier::Wasm => {
            std::fs::copy(manifest_path(from), to.join(MANIFEST_FILE)).map_err(io)?;
            let source = manifest.entry_path();
            let real = |p: &Path| p.canonicalize().map_err(io);
            if !real(&source)?.starts_with(real(from)?) {
                return Err(PluginError::Runtime(format!(
                    "{name}: entry {} is outside the plugin's folder",
                    source.display()
                )));
            }
            let entry = to.join(&manifest.plugin.entry);
            if let Some(parent) = entry.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            std::fs::copy(source, entry).map_err(io)?;
        }
        Tier::Python => copy_tree(from, &to, 0).map_err(io)?,
    }
    Ok(to)
}

/// Copies a directory tree, without `target` and hidden entries; at most
/// 16 levels deep (an exit clause against link loops).
fn copy_tree(from: &Path, to: &Path, depth: usize) -> std::io::Result<()> {
    if depth > 16 {
        return Err(std::io::Error::other(
            "directory tree deeper than 16 levels",
        ));
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if text.starts_with('.') || text == "target" || text == "__pycache__" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(&name), depth + 1)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// Exit clauses applied to every plugin call.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum wall-clock time of one call (and of instantiation).
    pub call_timeout: Duration,
    /// Maximum linear memory of a plugin instance, in bytes.
    pub max_memory_bytes: usize,
}

impl Limits {
    /// The most time a manifest may ask for one call.
    pub const MAX_TIMEOUT: Duration = Duration::from_secs(600);
    /// The most memory a manifest may ask for (WebAssembly 32-bit: 4 GiB).
    pub const MAX_MEMORY_BYTES: usize = 4 << 30;

    /// These limits raised to what `manifest` asks for, as far as the user
    /// allowed it for this plugin version in the plugin manager
    /// ([`consent_for`]; nothing allowed: not raised) — a plugin does not
    /// raise its own limits (security review 2026-10-26, M6). Never
    /// lowered; capped.
    pub fn for_manifest(self, manifest: &Manifest) -> Self {
        let consent = consent_for(&manifest.plugin.name, &manifest.plugin.version);
        self.raised(&manifest.limits, &consent.unwrap_or_default())
    }

    /// These limits raised to what is `asked`, at most to what is
    /// `allowed`.
    pub fn raised(self, asked: &RequestedLimits, allowed: &Consent) -> Self {
        let mut l = self;
        if let Some(s) = asked.seconds.zip(allowed.seconds).map(|(a, b)| a.min(b)) {
            l.call_timeout = l
                .call_timeout
                .max(Duration::from_secs(s).min(Self::MAX_TIMEOUT));
        }
        if let Some(m) = asked
            .memory_mib
            .zip(allowed.memory_mib)
            .map(|(a, b)| a.min(b))
        {
            let bytes = usize::try_from(m.saturating_mul(1 << 20)).unwrap_or(usize::MAX);
            l.max_memory_bytes = l.max_memory_bytes.max(bytes.min(Self::MAX_MEMORY_BYTES));
        }
        l
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            call_timeout: Duration::from_secs(5),
            max_memory_bytes: 256 * 1024 * 1024,
        }
    }
}

/// Resolution of the plugin time limit.
const EPOCH_TICK: Duration = Duration::from_millis(10);

impl Limits {
    /// Epoch ticks until a call is interrupted.
    ///
    /// The ticker runs independently of calls, so its first tick can come
    /// right after a call starts. One extra tick guarantees the plugin gets
    /// at least `call_timeout` (the remaining ticks are each ≥ one
    /// [`EPOCH_TICK`] apart); it is interrupted at most about one tick later.
    fn deadline_ticks(&self) -> u64 {
        self.call_timeout
            .as_millis()
            .div_ceil(EPOCH_TICK.as_millis()) as u64
            + 1
    }
}

/// Background thread advancing the engine's epoch every [`EPOCH_TICK`];
/// running plugin code is interrupted when its deadline epoch is reached.
struct EpochTicker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: &Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let engine = engine.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("ohl-plugin-epoch".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(EPOCH_TICK);
                        engine.increment_epoch();
                    }
                })
                .expect("spawn plugin epoch thread")
        };
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: Budget,
}

/// Core instances a plugin's store may hold (a component is a few).
pub const MAX_INSTANCES: usize = 64;
/// Linear memories a plugin's store may hold.
pub const MAX_MEMORIES: usize = 16;
/// Tables a plugin's store may hold.
pub const MAX_TABLES: usize = 64;
/// Elements of one table.
pub const MAX_TABLE_ELEMENTS: usize = 1 << 20;

/// What a plugin's store may take: its memories together at most
/// [`Limits::max_memory_bytes`] (not each), and a few instances, memories
/// and tables (security review 2026-10-26, M6).
struct Budget {
    max_memory: usize,
    used: usize,
}

impl Budget {
    fn new(max_memory: usize) -> Self {
        Self {
            max_memory,
            used: 0,
        }
    }
}

impl ResourceLimiter for Budget {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let more = desired.saturating_sub(current);
        if self.used.saturating_add(more) > self.max_memory {
            return Ok(false);
        }
        self.used += more;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= MAX_TABLE_ELEMENTS)
    }

    fn instances(&self) -> usize {
        MAX_INSTANCES
    }

    fn tables(&self) -> usize {
        MAX_TABLES
    }

    fn memories(&self) -> usize {
        MAX_MEMORIES
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// Loads and runs WebAssembly plugins. Create one per application and reuse it.
pub struct WasmHost {
    engine: Engine,
    linker: Linker<HostState>,
    limits: Limits,
    _ticker: EpochTicker,
}

impl WasmHost {
    /// Creates a host with a sandboxed WASI environment and default [`Limits`].
    pub fn new() -> Result<Self, PluginError> {
        Self::with_limits(Limits::default())
    }

    /// Creates a host with the given limits.
    pub fn with_limits(limits: Limits) -> Result<Self, PluginError> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        let ticker = EpochTicker::start(&engine);
        Ok(Self {
            engine,
            linker,
            limits,
            _ticker: ticker,
        })
    }

    /// Limits applied to plugin calls.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Reads and checks the manifest in `dir` (WebAssembly tier,
    /// compatibility, permissions and, if given, the extension point the
    /// plugin must list, with what to call it), compiles the component and
    /// sets up a sandboxed store with the limits the manifest asks for.
    fn prepare(&self, dir: &Path, point: Option<(&str, &str)>) -> Result<Prepared, PluginError> {
        let manifest = Manifest::load(dir)?;
        if manifest.plugin.tier != Tier::Wasm {
            return Err(PluginError::UnsupportedTier(
                manifest.plugin.tier.to_string(),
            ));
        }
        manifest.check_compatible()?;
        manifest.check_permissions()?;
        if let Some((point, kind)) = point
            && !manifest.plugin.extension_points.iter().any(|e| e == point)
        {
            return Err(PluginError::Manifest {
                path: manifest_path(dir),
                message: format!("{kind} lists the extension point \"{point}\""),
            });
        }
        let limits = self.limits.for_manifest(&manifest);
        let component = Component::from_file(&self.engine, manifest.entry_path())?;
        // No stdio, environment, file system or network: a fully sandboxed guest.
        let state = HostState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: Budget::new(limits.max_memory_bytes),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(limits.deadline_ticks());
        // Files, models and surfaces cross between host and plugin: as
        // much as its memory may hold.
        store.set_hostcall_fuel(limits.max_memory_bytes);
        Ok(Prepared {
            manifest,
            component,
            store,
            limits,
        })
    }

    /// Loads the plugin in `dir`: reads its manifest, checks compatibility
    /// and permissions, compiles and instantiates it, with the limits its
    /// manifest asks for.
    pub fn load(&self, dir: &Path) -> Result<WasmPlugin, PluginError> {
        let mut p = self.prepare(dir, None)?;
        let instance = bindings::Plugin::instantiate(&mut p.store, &p.component, &self.linker)
            .map_err(|e| classify(e, &p.manifest.plugin.name, "instantiation", p.limits))?;
        Ok(WasmPlugin {
            manifest: p.manifest,
            store: p.store,
            instance,
            limits: p.limits,
        })
    }

    /// Loads a file-format plugin (`io-plugin` world, extension point
    /// `io` in its manifest), with the limits its manifest asks for.
    pub fn load_io(&self, dir: &Path) -> Result<WasmIoPlugin, PluginError> {
        let mut p = self.prepare(dir, Some(("io", "a file-format plugin")))?;
        let instance = io_bindings::IoPlugin::instantiate(&mut p.store, &p.component, &self.linker)
            .map_err(|e| classify(e, &p.manifest.plugin.name, "instantiation", p.limits))?;
        Ok(WasmIoPlugin {
            manifest: p.manifest,
            store: p.store,
            instance,
            limits: p.limits,
        })
    }

    /// Loads a terrain plugin (`terrain-plugin` world, extension point
    /// `terrain` in its manifest), with the limits its manifest asks for.
    pub fn load_terrain(&self, dir: &Path) -> Result<WasmTerrainPlugin, PluginError> {
        let mut p = self.prepare(dir, Some(("terrain", "a terrain plugin")))?;
        let instance =
            terrain_bindings::TerrainPlugin::instantiate(&mut p.store, &p.component, &self.linker)
                .map_err(|e| classify(e, &p.manifest.plugin.name, "instantiation", p.limits))?;
        Ok(WasmTerrainPlugin {
            manifest: p.manifest,
            store: p.store,
            instance,
            limits: p.limits,
        })
    }

    /// Loads a project plugin (`project-plugin` world, extension point
    /// `project` in its manifest), with the limits its manifest asks for.
    pub fn load_project(&self, dir: &Path) -> Result<WasmProjectPlugin, PluginError> {
        let mut p = self.prepare(dir, Some(("project", "a project plugin")))?;
        let instance =
            project_bindings::ProjectPlugin::instantiate(&mut p.store, &p.component, &self.linker)
                .map_err(|e| classify(e, &p.manifest.plugin.name, "instantiation", p.limits))?;
        Ok(WasmProjectPlugin {
            manifest: p.manifest,
            store: p.store,
            instance,
            limits: p.limits,
        })
    }

    /// Loads a design-code plugin (`code-plugin` world, extension point
    /// [`DESIGN_CODE`] in its manifest), with the limits its manifest asks
    /// for.
    /// A plugin that also exports `structure-wind` (the
    /// `code-wind-plugin` world) is loaded with it; one built without it
    /// loads as before and gives no wind factors. The same for the
    /// optional `rule-presets` export (`code-rules-plugin`): looked for on
    /// its own, none without it.
    pub fn load_code(&self, dir: &Path) -> Result<WasmCodePlugin, PluginError> {
        let mut p = self.prepare(dir, Some((DESIGN_CODE, "a design-code plugin")))?;
        let fail =
            |e, p: &Prepared| classify(e, &p.manifest.plugin.name, "instantiation", p.limits);
        let has = |name: &str| p.component.get_export_index(None, name).is_some();
        let (wind, rules) = (has(STRUCTURE_WIND_EXPORT), has(RULE_PRESETS_EXPORT));
        let component_instance = self
            .linker
            .instantiate(&mut p.store, &p.component)
            .map_err(|e| fail(e, &p))?;
        let instance = if wind {
            CodeInstance::Wind(
                code_wind_bindings::CodeWindPlugin::new(&mut p.store, &component_instance)
                    .map_err(|e| fail(e, &p))?,
            )
        } else {
            CodeInstance::Plain(
                code_bindings::CodePlugin::new(&mut p.store, &component_instance)
                    .map_err(|e| fail(e, &p))?,
            )
        };
        let rule_presets = if rules {
            Some(
                rule_presets_bindings::RulePresetsExport::new(&mut p.store, &component_instance)
                    .map_err(|e| fail(e, &p))?,
            )
        } else {
            None
        };
        Ok(WasmCodePlugin {
            manifest: p.manifest,
            store: p.store,
            instance,
            rule_presets,
            limits: p.limits,
        })
    }
}

/// A design-code plugin's instance: with the `structure-wind` export or
/// without.
enum CodeInstance {
    Plain(code_bindings::CodePlugin),
    Wind(code_wind_bindings::CodeWindPlugin),
}

/// A loaded design-code plugin.
pub struct WasmCodePlugin {
    manifest: Manifest,
    store: Store<HostState>,
    instance: CodeInstance,
    /// The optional `rule-presets` export.
    rule_presets: Option<rule_presets_bindings::RulePresetsExport>,
    limits: Limits,
}

impl WasmCodePlugin {
    /// The plugin's manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Calls `describe`.
    pub fn describe(&mut self) -> Result<CodeInfo, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        match &self.instance {
            CodeInstance::Plain(i) => i.call_describe(&mut self.store),
            CodeInstance::Wind(i) => i.call_describe(&mut self.store).map(code_info),
        }
        .map_err(|e| classify(e, &name, "describe", limits))
    }

    /// Calls `generate` with a parameter file's text (inner `Err`: the
    /// plugin's own error, about the file).
    pub fn generate(&mut self, parameters: &str) -> Result<Result<Generated, String>, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        match &self.instance {
            CodeInstance::Plain(i) => i.call_generate(&mut self.store, parameters),
            CodeInstance::Wind(i) => i
                .call_generate(&mut self.store, parameters)
                .map(|r| r.map(generated)),
        }
        .map_err(|e| classify(e, &name, "generate", limits))
    }

    /// Whether the plugin exports `structure-wind` (built for it).
    pub fn has_structure_wind(&self) -> bool {
        matches!(self.instance, CodeInstance::Wind(_))
    }

    /// Calls `structure-wind` with a parameter file's text: the code's
    /// wind factors for structures as a section library's text with a
    /// `[wind]` table, or none (also for a plugin built without the
    /// export). Inner `Err`: the plugin's own error, about the file.
    pub fn structure_wind(
        &mut self,
        parameters: &str,
    ) -> Result<Result<Option<String>, String>, PluginError> {
        let CodeInstance::Wind(i) = &self.instance else {
            return Ok(Ok(None));
        };
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        i.ohl_plugin_structure_wind()
            .call_structure_wind(&mut self.store, parameters)
            .map_err(|e| classify(e, &name, "structure-wind", limits))
    }

    /// Whether the plugin exports `rule-presets` (built for it).
    pub fn has_rule_presets(&self) -> bool {
        self.rule_presets.is_some()
    }

    /// Calls `rule-presets` with a parameter file's text (empty: none
    /// given): the code's presets of clearance rules, none for a plugin
    /// built without the export. Inner `Err`: the plugin's own error,
    /// about the file.
    pub fn rule_presets(
        &mut self,
        parameters: &str,
    ) -> Result<Result<Vec<RulePreset>, String>, PluginError> {
        let Some(i) = &self.rule_presets else {
            return Ok(Ok(Vec::new()));
        };
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        i.ohl_plugin_rule_presets()
            .call_rule_presets(&mut self.store, parameters)
            .map_err(|e| classify(e, &name, "rule-presets", limits))
    }
}

/// The types of the `code-wind-plugin` bindings are the same WIT types as
/// those of `code-plugin`, generated twice: converted field by field.
fn code_info(c: code_wind_bindings::ohl::plugin::code_types::CodeInfo) -> CodeInfo {
    CodeInfo {
        name: c.name,
        version: c.version,
        description: c.description,
        parameters_template: c.parameters_template,
    }
}

fn generated(g: code_wind_bindings::ohl::plugin::code_types::Generated) -> Generated {
    use code_wind_bindings::ohl::plugin::code_types::ClearanceMeasure as M;
    Generated {
        weather_cases: g.weather_cases,
        load_cases: g.load_cases,
        notes: g.notes,
        clearance_presets: g
            .clearance_presets
            .into_iter()
            .map(|p| ClearancePreset {
                name: p.name,
                measure: match p.measure {
                    M::Vertical => ClearanceMeasure::Vertical,
                    M::Radial => ClearanceMeasure::Radial,
                },
                rules: p.rules,
                weather_cases: p.weather_cases,
            })
            .collect(),
    }
}

impl WasmHost {
    /// Loads a check plugin (`check-plugin` world, extension point
    /// [`CHECK`] in its manifest), with the limits its manifest asks for.
    pub fn load_check(&self, dir: &Path) -> Result<WasmCheckPlugin, PluginError> {
        let p = self.prepare(dir, Some((CHECK, "a check plugin")))?;
        self.check_instance(p.manifest, p.component, p.store, p.limits)
    }

    fn check_instance(
        &self,
        manifest: Manifest,
        component: Component,
        mut store: Store<HostState>,
        limits: Limits,
    ) -> Result<WasmCheckPlugin, PluginError> {
        let instance =
            check_bindings::CheckPlugin::instantiate(&mut store, &component, &self.linker)
                .map_err(|e| classify(e, &manifest.plugin.name, "instantiation", limits))?;
        Ok(WasmCheckPlugin {
            manifest,
            component,
            store,
            instance,
            limits,
        })
    }

    /// Another instance of check plugin `plugin` (its own store: one per
    /// thread), without compiling it again.
    pub fn another_check(&self, plugin: &WasmCheckPlugin) -> Result<WasmCheckPlugin, PluginError> {
        let state = HostState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: Budget::new(plugin.limits.max_memory_bytes),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(plugin.limits.deadline_ticks());
        store.set_hostcall_fuel(plugin.limits.max_memory_bytes);
        self.check_instance(
            plugin.manifest.clone(),
            plugin.component.clone(),
            store,
            plugin.limits,
        )
    }
}

/// A loaded check plugin.
pub struct WasmCheckPlugin {
    manifest: Manifest,
    component: Component,
    store: Store<HostState>,
    instance: check_bindings::CheckPlugin,
    limits: Limits,
}

impl WasmCheckPlugin {
    /// The plugin's manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn call<T>(
        &mut self,
        function: &'static str,
        f: impl FnOnce(
            &check_bindings::exports::ohl::plugin::check::Guest,
            &mut Store<HostState>,
        ) -> wasmtime::Result<T>,
    ) -> Result<T, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let (name, limits) = (self.manifest.plugin.name.clone(), self.limits);
        f(self.instance.ohl_plugin_check(), &mut self.store)
            .map_err(|e| classify(e, &name, function, limits))
    }

    /// Calls `checks`.
    pub fn checks(&mut self) -> Result<Vec<CheckInfo>, PluginError> {
        self.call("checks", |g, s| g.call_checks(s))
    }

    /// Calls `value-tables` (inner `Err`: the plugin's own error, about the
    /// values).
    pub fn value_tables(
        &mut self,
        check: &str,
        values: &str,
    ) -> Result<Result<Vec<ValueTable>, String>, PluginError> {
        self.call("value-tables", |g, s| g.call_value_tables(s, check, values))
    }

    /// Calls `edit-value`: the values file with that cell changed.
    pub fn edit_value(
        &mut self,
        check: &str,
        values: &str,
        cell: [u32; 3],
        text: &str,
    ) -> Result<Result<String, String>, PluginError> {
        self.call("edit-value", |g, s| {
            g.call_edit_value(s, check, values, cell[0], cell[1], cell[2], text)
        })
    }

    /// Calls `evaluate` for one pair and its stations.
    pub fn evaluate(
        &mut self,
        check: &str,
        values: &str,
        choices: &[u32],
        pair: &ConductorPair,
        stations: &[f64],
    ) -> Result<Result<Evaluated, String>, PluginError> {
        self.call("evaluate", |g, s| {
            g.call_evaluate(s, check, values, choices, pair, stations)
        })
    }
}

/// The check plugins in `roots` (each as [`discover`] reads it): those
/// whose manifest lists [`CHECK`], usable or not; a name found again is
/// left out.
pub fn discover_checks(roots: &[PathBuf]) -> Vec<Found> {
    let mut names = std::collections::BTreeSet::new();
    roots
        .iter()
        .flat_map(|r| discover(r))
        .filter(|f| f.provides(CHECK))
        .filter(|f| names.insert(f.name()))
        .collect()
}

/// The plan sets plugin `dir`'s manifest names (`[[plan-set]]`): each
/// file's name (without its extension) and text.
pub fn plan_sets(dir: &Path) -> Result<Vec<(String, String)>, PluginError> {
    let manifest = Manifest::load(dir)?;
    let mut out = Vec::new();
    for t in &manifest.plan_sets {
        let path = dir.join(&t.file);
        let text = std::fs::read_to_string(&path).map_err(|e| PluginError::Manifest {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let name = t
            .file
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        out.push((name, text));
    }
    Ok(out)
}

/// The plugins in `roots` giving sheet templates (each as [`discover`]
/// reads it): those whose manifest lists [`SHEET_TEMPLATES`] or names
/// template files; a name found again is left out.
pub fn discover_sheet_templates(roots: &[PathBuf]) -> Vec<Found> {
    let mut names = std::collections::BTreeSet::new();
    roots
        .iter()
        .flat_map(|r| discover(r))
        .filter(|f| {
            f.provides(SHEET_TEMPLATES)
                || f.manifest
                    .as_ref()
                    .is_ok_and(|m| !m.sheet_templates.is_empty() || !m.plan_sets.is_empty())
        })
        .filter(|f| names.insert(f.name()))
        .collect()
}

/// The design-code plugins in `roots` (each as [`discover`] reads it):
/// those whose manifest lists [`DESIGN_CODE`], usable or not; a name
/// found again is left out.
pub fn discover_codes(roots: &[PathBuf]) -> Vec<Found> {
    let mut names = std::collections::BTreeSet::new();
    roots
        .iter()
        .flat_map(|r| discover(r))
        .filter(|f| f.provides(DESIGN_CODE))
        .filter(|f| names.insert(f.name()))
        .collect()
}

/// What a plugin of any world says about itself
/// ([`WasmHost::describe_any`]).
#[derive(Debug, Clone)]
pub struct Description {
    /// The worlds it was loaded in (`code-plugin`, `io-plugin`,
    /// `terrain-plugin`, `project-plugin`, `check-plugin` or the minimal
    /// `plugin`).
    pub worlds: Vec<&'static str>,
    /// Name, version and description from its `describe` (from the
    /// manifest for a check plugin, which has none).
    pub info: PluginInfo,
    /// What else it offers, one line each: its formats, checks, the size of
    /// its parameter template.
    pub details: Vec<String>,
}

/// "format geojson: GeoJSON (.geojson, importer)".
fn format_line(
    id: &str,
    description: &str,
    extension: &str,
    direction: Option<Direction>,
) -> String {
    let direction = match direction {
        Some(Direction::Importer) => ", importer",
        Some(Direction::Exporter) => ", exporter",
        None => "",
    };
    format!("format {id}: {description} (.{extension}{direction})")
}

impl WasmHost {
    /// The plan sets plugin `dir`'s manifest names (`[[plan-set]]`): each
    /// file's name (without its extension) and text, read as they are
    /// (the caller checks them).
    pub fn plan_sets(&self, dir: &Path) -> Result<Vec<(String, String)>, PluginError> {
        plan_sets(dir)
    }

    /// The sheet templates of the plugin in `dir`: the files its manifest
    /// names (read as they are, no code run), then those of its
    /// `sheet-templates` export when it is a WebAssembly plugin built with
    /// it (one built without it, or before it, gives none). Its own error
    /// (why it has none) comes back as an `Err` naming it.
    pub fn sheet_templates(&self, dir: &Path) -> Result<Vec<TemplatePreset>, PluginError> {
        let manifest = Manifest::load(dir)?;
        let mut out = Vec::new();
        for t in &manifest.sheet_templates {
            let path = dir.join(&t.file);
            let text = std::fs::read_to_string(&path).map_err(|e| PluginError::Manifest {
                path: path.clone(),
                message: e.to_string(),
            })?;
            out.push(TemplatePreset {
                name: t
                    .file
                    .file_stem()
                    .map_or(String::new(), |s| s.to_string_lossy().into_owned()),
                description: String::new(),
                template: text,
                images: Vec::new(),
                dir: path.parent().map(Path::to_path_buf),
            });
        }
        // Code: only a WebAssembly plugin whose entry is there.
        if manifest.plugin.tier != Tier::Wasm || !manifest.entry_path().is_file() {
            return Ok(out);
        }
        let mut p = self.prepare(dir, None)?;
        if p.component
            .get_export_index(None, SHEET_TEMPLATES_EXPORT)
            .is_none()
        {
            return Ok(out);
        }
        let name = p.manifest.plugin.name.clone();
        let fail = |e, p: &Prepared, what| classify(e, &p.manifest.plugin.name, what, p.limits);
        let instance = self
            .linker
            .instantiate(&mut p.store, &p.component)
            .map_err(|e| fail(e, &p, "instantiation"))?;
        let export = sheet_templates_bindings::SheetTemplatesExport::new(&mut p.store, &instance)
            .map_err(|e| fail(e, &p, "instantiation"))?;
        p.store.set_epoch_deadline(p.limits.deadline_ticks());
        let given = export
            .ohl_plugin_sheet_templates()
            .call_sheet_templates(&mut p.store)
            .map_err(|e| classify(e, &name, "sheet-templates", p.limits))?
            .map_err(|e| PluginError::Runtime(format!("plugin {name}: {e}")))?;
        out.extend(given.into_iter().map(|t| {
            TemplatePreset {
                name: t.name,
                description: t.description,
                template: t.template,
                images: t
                    .images
                    .into_iter()
                    .map(|i| (i.name, i.media_type, i.bytes))
                    .collect(),
                dir: None,
            }
        }));
        Ok(out)
    }

    /// Loads the plugin in `dir` in the world of each extension point its
    /// manifest lists (`design-code`, `io`, `terrain`, `project`, `check`;
    /// none of these: the minimal `plugin` world) and says what it
    /// describes: `ohl plugin info`.
    pub fn describe_any(&self, dir: &Path) -> Result<Description, PluginError> {
        let manifest = Manifest::load(dir)?;
        let has = |p: &str| manifest.plugin.extension_points.iter().any(|e| e == p);
        let mut worlds = Vec::new();
        let mut info: Option<PluginInfo> = None;
        let mut details = Vec::new();
        if has(DESIGN_CODE) {
            let mut code = self.load_code(dir)?;
            let c = code.describe()?;
            details.push(format!(
                "parameter template: {} lines",
                c.parameters_template.lines().count()
            ));
            let optional: Vec<&str> = [
                (code.has_structure_wind(), "structure-wind"),
                (code.has_rule_presets(), "rule-presets"),
            ]
            .into_iter()
            .filter_map(|(on, n)| on.then_some(n))
            .collect();
            if !optional.is_empty() {
                details.push(format!("optional exports: {}", optional.join(", ")));
            }
            info.get_or_insert(PluginInfo {
                name: c.name,
                version: c.version,
                description: c.description,
            });
            worlds.push("code-plugin");
        }
        if has("io") {
            let mut p = self.load_io(dir)?;
            let i = p.describe()?;
            for f in p.formats()? {
                details.push(format_line(
                    &f.id,
                    &f.description,
                    &f.extension,
                    Some(f.direction),
                ));
            }
            info.get_or_insert(i);
            worlds.push("io-plugin");
        }
        if has("terrain") {
            let mut p = self.load_terrain(dir)?;
            let i = p.describe()?;
            for f in p.formats()? {
                details.push(format_line(
                    &f.id,
                    &f.description,
                    &f.extension,
                    Some(f.direction),
                ));
            }
            info.get_or_insert(i);
            worlds.push("terrain-plugin");
        }
        if has("project") {
            let mut p = self.load_project(dir)?;
            let i = p.describe()?;
            for (id, description, extension) in p.formats()? {
                details.push(format_line(&id, &description, &extension, None));
            }
            info.get_or_insert(i);
            worlds.push("project-plugin");
        }
        if has(CHECK) {
            for c in self.load_check(dir)?.checks()? {
                details.push(format!("check {}: {}", c.id, c.title));
            }
            worlds.push("check-plugin");
        }
        if worlds.is_empty() {
            info = Some(self.load(dir)?.describe()?);
            worlds.push("plugin");
        }
        let info = info.unwrap_or_else(|| PluginInfo {
            name: manifest.plugin.name.clone(),
            version: manifest.plugin.version.clone(),
            description: manifest.plugin.description.clone(),
        });
        Ok(Description {
            worlds,
            info,
            details,
        })
    }
}

/// A checked plugin, compiled, with its store ([`WasmHost::prepare`]).
struct Prepared {
    manifest: Manifest,
    component: Component,
    store: Store<HostState>,
    limits: Limits,
}

/// A loaded project plugin.
pub struct WasmProjectPlugin {
    manifest: Manifest,
    store: Store<HostState>,
    instance: project_bindings::ProjectPlugin,
    limits: Limits,
}

impl WasmProjectPlugin {
    /// The plugin's manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Calls `describe`.
    pub fn describe(&mut self) -> Result<PluginInfo, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        let info = self
            .instance
            .call_describe(&mut self.store)
            .map_err(|e| classify(e, &name, "describe", limits))?;
        // Same WIT record, generated once per world.
        Ok(PluginInfo {
            name: info.name,
            version: info.version,
            description: info.description,
        })
    }

    /// Calls `formats` (their id, description, extension).
    pub fn formats(&mut self) -> Result<Vec<(String, String, String)>, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        let formats = self
            .instance
            .call_formats(&mut self.store)
            .map_err(|e| classify(e, &name, "formats", limits))?;
        Ok(formats
            .into_iter()
            .map(|f| (f.id, f.description, f.extension))
            .collect())
    }

    /// Calls `import-project` (inner `Err`: the plugin's own error).
    pub fn import_project(
        &mut self,
        format: &str,
        data: &[u8],
        options: &[(String, String)],
    ) -> Result<Result<Project, String>, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        self.instance
            .call_import_project(&mut self.store, format, data, options)
            .map_err(|e| classify(e, &name, "import-project", limits))
    }
}

/// A loaded terrain plugin.
pub struct WasmTerrainPlugin {
    manifest: Manifest,
    store: Store<HostState>,
    instance: terrain_bindings::TerrainPlugin,
    limits: Limits,
}

impl WasmTerrainPlugin {
    /// The plugin's manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn arm(&mut self) {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
    }

    /// Calls `describe`.
    pub fn describe(&mut self) -> Result<PluginInfo, PluginError> {
        self.arm();
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        let info = self
            .instance
            .call_describe(&mut self.store)
            .map_err(|e| classify(e, &name, "describe", limits))?;
        // Same WIT record, generated once per world.
        Ok(PluginInfo {
            name: info.name,
            version: info.version,
            description: info.description,
        })
    }

    /// Calls `formats`.
    pub fn formats(&mut self) -> Result<Vec<Format>, PluginError> {
        self.arm();
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        let formats = self
            .instance
            .call_formats(&mut self.store)
            .map_err(|e| classify(e, &name, "formats", limits))?;
        // Same WIT record, generated once per world.
        Ok(formats
            .into_iter()
            .map(|f| Format {
                id: f.id,
                description: f.description,
                extension: f.extension,
                direction: match f.direction {
                    terrain_bindings::ohl::plugin::io_types::Direction::Exporter => {
                        Direction::Exporter
                    }
                    terrain_bindings::ohl::plugin::io_types::Direction::Importer => {
                        Direction::Importer
                    }
                },
            })
            .collect())
    }

    /// Calls `import-surfaces` (inner `Err`: the plugin's own error).
    pub fn import_surfaces(
        &mut self,
        format: &str,
        data: &[u8],
        options: &[(String, String)],
    ) -> Result<Result<Vec<Surface>, String>, PluginError> {
        self.arm();
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        self.instance
            .call_import_surfaces(&mut self.store, format, data, options)
            .map_err(|e| classify(e, &name, "import-surfaces", limits))
    }
}

/// A loaded file-format plugin.
pub struct WasmIoPlugin {
    manifest: Manifest,
    store: Store<HostState>,
    instance: io_bindings::IoPlugin,
    limits: Limits,
}

impl WasmIoPlugin {
    /// The plugin's manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn arm(&mut self) {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
    }

    fn classify(&self, error: wasmtime::Error, function: &'static str) -> PluginError {
        classify(error, &self.manifest.plugin.name, function, self.limits)
    }

    /// Calls `describe`.
    pub fn describe(&mut self) -> Result<PluginInfo, PluginError> {
        self.arm();
        let info = self
            .instance
            .call_describe(&mut self.store)
            .map_err(|e| self.classify(e, "describe"))?;
        // Same WIT record, generated once per world.
        Ok(PluginInfo {
            name: info.name,
            version: info.version,
            description: info.description,
        })
    }

    /// Calls `formats`.
    pub fn formats(&mut self) -> Result<Vec<Format>, PluginError> {
        self.arm();
        self.instance
            .call_formats(&mut self.store)
            .map_err(|e| self.classify(e, "formats"))
    }

    /// Calls `export-model`; the plugin's own error comes back as `Err` in
    /// the inner result.
    pub fn export_model(
        &mut self,
        format: &str,
        model: &str,
        options: &[(String, String)],
    ) -> Result<Result<Vec<u8>, String>, PluginError> {
        self.arm();
        self.instance
            .call_export_model(&mut self.store, format, model, options)
            .map_err(|e| self.classify(e, "export-model"))
    }

    /// Calls `import-file`; returns the line file JSON (inner `Err`: the
    /// plugin's own error).
    pub fn import_file(
        &mut self,
        format: &str,
        data: &[u8],
        options: &[(String, String)],
    ) -> Result<Result<String, String>, PluginError> {
        self.arm();
        self.instance
            .call_import_file(&mut self.store, format, data, options)
            .map_err(|e| self.classify(e, "import-file"))
    }
}

/// A loaded WebAssembly plugin instance.
pub struct WasmPlugin {
    manifest: Manifest,
    store: Store<HostState>,
    instance: bindings::Plugin,
    limits: Limits,
}

/// Turns a runtime error into [`PluginError::Timeout`] when it is an epoch
/// interruption, so callers learn *why* the call stopped.
fn classify(
    error: wasmtime::Error,
    plugin: &str,
    function: &'static str,
    limits: Limits,
) -> PluginError {
    if error.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
        PluginError::Timeout {
            plugin: plugin.to_owned(),
            function,
            limit: limits.call_timeout,
        }
    } else {
        error.into()
    }
}

impl WasmPlugin {
    /// The plugin's manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Resets the time limit before a call.
    fn arm(&mut self) {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
    }

    fn classify(&self, error: wasmtime::Error, function: &'static str) -> PluginError {
        classify(error, &self.manifest.plugin.name, function, self.limits)
    }

    /// Calls the plugin's `describe` export.
    pub fn describe(&mut self) -> Result<PluginInfo, PluginError> {
        self.arm();
        self.instance
            .call_describe(&mut self.store)
            .map_err(|e| self.classify(e, "describe"))
    }

    /// Calls the plugin's `span-load` export.
    pub fn span_load(&mut self, load_per_meter: f64, span_length: f64) -> Result<f64, PluginError> {
        self.arm();
        self.instance
            .call_span_load(&mut self.store, load_per_meter, span_length)
            .map_err(|e| self.classify(e, "span-load"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(sdk: &str, extra: &str) -> Manifest {
        let text = format!(
            "[plugin]\nname = \"t\"\nversion = \"1.0.0\"\nsdk = \"{sdk}\"\ntier = \"wasm\"\nentry = \"t.wasm\"\n{extra}"
        );
        Manifest::parse(&text, Path::new("/plugins/t")).unwrap()
    }

    #[test]
    fn parses_manifest() {
        let m = manifest("0.3", "extension-points = [\"checks\"]");
        assert_eq!(m.plugin.tier, Tier::Wasm);
        assert_eq!(m.plugin.extension_points, ["checks"]);
        assert_eq!(m.entry_path(), Path::new("/plugins/t/t.wasm"));
        assert!(!m.permissions.network);
    }

    /// `[[column]]`s: read, keyed `plugin.column`; a table, name or
    /// default not allowed is refused.
    #[test]
    fn table_columns() {
        let m = manifest(
            "0.3",
            "[[column]]\ntable = \"circuits\"\nname = \"level\"\nvalues = [\"low\", \"high\"]\ndefault = \"high\"",
        );
        assert_eq!(m.columns.len(), 1);
        assert_eq!(m.field_key(&m.columns[0]), "t.level");
        assert_eq!(m.columns[0].values, ["low", "high"]);
        for bad in [
            "table = \"nowhere\"\nname = \"x\"",
            "table = \"circuits\"\nname = \"a.b\"",
            "table = \"circuits\"\nname = \"x\"\nvalues = [\"a\"]\ndefault = \"b\"",
        ] {
            let text = format!(
                "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"wasm\"\nentry=\"t.wasm\"\n[[column]]\n{bad}"
            );
            assert!(
                Manifest::parse(&text, Path::new("/plugins/t")).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_entry_stays_inside_the_plugin_folder() {
        for entry in ["../x.wasm", "a/../../x.wasm", "/etc/x.wasm", ""] {
            let text = format!(
                "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"wasm\"\nentry=\"{entry}\""
            );
            assert!(
                Manifest::parse(&text, Path::new("/plugins/t")).is_err(),
                "{entry}"
            );
        }
        let text =
            "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"wasm\"\nentry=\"bin/t.wasm\"";
        assert!(Manifest::parse(text, Path::new("/plugins/t")).is_ok());
    }

    #[test]
    fn rejects_unknown_fields() {
        let text = "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"wasm\"\nentry=\"t.wasm\"\nunknown=1";
        assert!(matches!(
            Manifest::parse(text, Path::new(".")),
            Err(PluginError::Manifest { .. })
        ));
    }

    #[test]
    fn compatibility_rules() {
        assert!(manifest("0.3", "").check_compatible().is_ok());
        // 0.1: before the design codes' clearance presets.
        assert!(manifest("0.1", "").check_compatible().is_err());
        // 0.2: the ols:plugin interface of before 2026-10-18, to rebuild.
        let old = manifest("0.2", "")
            .check_compatible()
            .unwrap_err()
            .to_string();
        assert!(
            old.contains("rebuild it for the ohl:plugin interface"),
            "{old}"
        );
        assert!(manifest("0.4", "").check_compatible().is_err());
        assert!(manifest("1.0", "").check_compatible().is_err());
        assert!(manifest("zero", "").check_compatible().is_err());
    }

    #[test]
    fn permissions_denied_by_default() {
        assert!(manifest("0.3", "").check_permissions().is_ok());
        let net = manifest("0.3", "[permissions]\nnetwork = true");
        assert!(matches!(
            net.check_permissions(),
            Err(PluginError::PermissionDenied { .. })
        ));
        let fs = manifest("0.3", "[permissions]\nfilesystem = [\"/tmp\"]");
        assert!(fs.check_permissions().is_err());
    }

    /// M6 and L1 (security review 2026-10-26): a manifest asking for more
    /// time or memory gets it only as far as the user allowed this
    /// version; a Python plugin asks to run unsandboxed.
    #[test]
    fn limits_and_python_need_consent() {
        let greedy = manifest("0.3", "[limits]\nseconds = 600\nmemory-mib = 4096");
        let d = Limits::default();
        let l = d.for_manifest(&greedy);
        assert_eq!(l.call_timeout, d.call_timeout, "not allowed: not raised");
        assert_eq!(l.max_memory_bytes, d.max_memory_bytes);
        let asked = greedy.asks();
        assert_eq!((asked.seconds, asked.memory_mib), (Some(600), Some(4096)));
        assert!(!asked.unsandboxed);
        assert_eq!(greedy.to_allow(None), Some(asked));
        let some = Consent {
            seconds: Some(60),
            memory_mib: Some(1024),
            ..Consent::default()
        };
        let l = d.raised(&greedy.limits, &some);
        assert_eq!(l.call_timeout, Duration::from_secs(60));
        assert_eq!(l.max_memory_bytes, 1024 << 20);
        assert!(greedy.to_allow(Some(&some)).is_some(), "asks more still");
        // Allowed for this version (the plugin manager's answer).
        set_consents(&[(greedy.consent_key(), asked)].into());
        let l = d.for_manifest(&greedy);
        assert_eq!(l.call_timeout, Duration::from_secs(600));
        assert_eq!(l.max_memory_bytes, 4096 << 20);
        assert_eq!(
            greedy.to_allow(consent_for("t", "1.0.0").as_ref()),
            None,
            "asked once"
        );
        assert_eq!(
            consent_for("t", "1.0.1"),
            None,
            "another version: asked again"
        );
        set_consents(&std::collections::BTreeMap::new());
        // Within the defaults: nothing to allow.
        assert_eq!(
            manifest("0.3", "[limits]\nseconds = 2").to_allow(None),
            None
        );
        // Python: unsandboxed, its permissions not "checked".
        let python = Manifest::parse(
            "[plugin]\nname = \"p\"\nversion = \"1\"\nsdk = \"0.3\"\ntier = \"python\"\n\
             entry = \"p.py\"\n[permissions]\nnetwork = true",
            Path::new("/plugins/p"),
        )
        .unwrap();
        let asked = python.asks();
        assert!(asked.unsandboxed);
        assert!(asked.describe().contains("not sandboxed"));
        assert!(python.to_allow(None).is_some());
        let found = Found {
            dir: PathBuf::from("/plugins/p"),
            manifest: Ok(python),
        };
        let problem = found.problem().unwrap_or_default();
        assert!(!problem.contains("network"), "{problem}");
    }

    /// M6: the memories of a store together, not each, within the limit;
    /// few instances, memories and tables; tables bounded.
    #[test]
    fn store_budget_counts_all_memories() {
        let mut b = Budget::new(100);
        assert!(b.memory_growing(0, 60, None).unwrap());
        assert!(!b.memory_growing(0, 60, None).unwrap(), "a second memory");
        assert!(b.memory_growing(0, 40, None).unwrap());
        assert!(!b.memory_growing(60, 61, None).unwrap());
        assert!(b.table_growing(0, MAX_TABLE_ELEMENTS, None).unwrap());
        assert!(!b.table_growing(0, MAX_TABLE_ELEMENTS + 1, None).unwrap());
        assert!(b.instances() <= 64 && b.memories() <= 16 && b.tables() <= 64);
    }

    #[test]
    fn deadline_never_shorter_than_limit() {
        for ms in [1, 10, 15, 200, 5000] {
            let limits = Limits {
                call_timeout: Duration::from_millis(ms),
                ..Limits::default()
            };
            let guaranteed = (limits.deadline_ticks() - 1) * EPOCH_TICK.as_millis() as u64;
            assert!(
                guaranteed >= ms,
                "{ms} ms -> {} ticks",
                limits.deadline_ticks()
            );
        }
    }

    #[test]
    fn plugins_are_found_and_installed() {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/examples");
        let found = discover(&examples);
        let names: Vec<String> = found.iter().map(Found::name).collect();
        for n in ["geojson", "hello-python", "hello-wasm"] {
            assert!(names.iter().any(|x| x == n), "{names:?}");
        }
        // One plugin directory: itself.
        assert_eq!(discover(&examples.join("hello-python")).len(), 1);
        assert!(discover(Path::new("/no/such/dir")).is_empty());
        // A Python plugin is installed with its files; again only replacing.
        let root = std::env::temp_dir().join(format!("ohl-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let to = install(&examples.join("hello-python"), &root, false).unwrap();
        assert!(to.join("hello.py").is_file());
        assert!(install(&examples.join("hello-python"), &root, false).is_err());
        install(&examples.join("hello-python"), &root, true).unwrap();
        let installed = discover(&root);
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].problem(), None);
        // Another interface version: found, not usable, not installed.
        let other = root.join("other-src");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(
            other.join(MANIFEST_FILE),
            "[plugin]\nname=\"other\"\nversion=\"1\"\nsdk=\"9.0\"\ntier=\"python\"\nentry=\"x.py\"",
        )
        .unwrap();
        std::fs::write(other.join("x.py"), "").unwrap();
        let f = discover(&other);
        assert!(
            f[0].problem().unwrap().contains("9.0"),
            "{:?}",
            f[0].problem()
        );
        assert!(install(&other, &root, false).is_err());
        // A manifest under its name of before 2026-10-18: found, read,
        // installed under the current name.
        let old = root.join("old-src");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join(LEGACY_MANIFEST_FILE),
            "[plugin]\nname=\"old\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"python\"\nentry=\"x.py\"",
        )
        .unwrap();
        std::fs::write(old.join("x.py"), "").unwrap();
        let f = discover(&old);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].problem(), None);
        let to = install(&old, &root, false).unwrap();
        assert!(to.join(MANIFEST_FILE).is_file() || to.join(LEGACY_MANIFEST_FILE).is_file());
        assert_eq!(discover(&to)[0].name(), "old");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn host_is_created() {
        assert!(WasmHost::new().is_ok());
    }

    /// Returns the plugin directory if its WebAssembly file has been built
    /// (CI builds it first; see CONTRIBUTING.md), `None` to skip the test.
    fn built_plugin(relative: &str) -> Option<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins")
            .join(relative);
        let manifest = Manifest::load(&dir).unwrap();
        if manifest.entry_path().exists() {
            return Some(dir);
        }
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugin not built: {}",
            manifest.entry_path().display()
        );
        eprintln!("skipping: {relative} not built");
        None
    }

    #[test]
    fn runs_example_plugin_when_built() {
        let Some(dir) = built_plugin("examples/hello-wasm") else {
            return;
        };
        let host = WasmHost::new().unwrap();
        let mut plugin = host.load(&dir).unwrap();
        let info = plugin.describe().unwrap();
        assert_eq!(info.name, "hello-wasm");
        assert_eq!(plugin.span_load(16.0, 400.0).unwrap(), 6400.0);
    }

    /// A plugin stuck in an endless loop must be interrupted at the time
    /// limit, report why, and leave the host usable.
    #[test]
    fn interrupts_endless_loop() {
        let Some(dir) = built_plugin("tests/runaway") else {
            return;
        };
        let limits = Limits {
            call_timeout: Duration::from_millis(200),
            ..Limits::default()
        };
        let host = WasmHost::with_limits(limits).unwrap();
        let mut plugin = host.load(&dir).unwrap();
        assert_eq!(plugin.describe().unwrap().name, "runaway");

        let started = std::time::Instant::now();
        let error = plugin.span_load(1.0, 1.0).unwrap_err();
        let elapsed = started.elapsed();
        assert!(
            matches!(
                error,
                PluginError::Timeout {
                    function: "span-load",
                    ..
                }
            ),
            "{error}"
        );
        // Never interrupted before the limit (regression: CI saw 193.9 ms).
        assert!(elapsed >= Duration::from_millis(200), "{elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");

        // The host still works after the interruption.
        let mut other = host.load(&dir).unwrap();
        assert_eq!(other.describe().unwrap().name, "runaway");
    }

    /// A plugin gives sheet templates from its `sheet-templates` export
    /// (with its images) and from its manifest's files (no code); a plugin
    /// built without the export (or before it) still loads and gives
    /// none.
    #[test]
    fn plugins_give_sheet_templates() {
        let Some(dir) = built_plugin("tests/sheet-presets") else {
            return;
        };
        let host = WasmHost::new().unwrap();
        let list = host.sheet_templates(&dir).unwrap();
        let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["test-a3", "test-a1"]);
        // The file: read as it is, its images next to it.
        assert!(list[0].template.contains("name = \"test-a3\""));
        assert_eq!(
            list[0].dir.as_deref(),
            Some(dir.join("templates").as_path())
        );
        // The export: its text and its logo.
        assert!(list[1].template.contains("format = \"ohl-sheet-template\""));
        assert_eq!(list[1].images.len(), 1);
        assert_eq!(list[1].images[0].0, "test-logo.svg");
        assert!(list[1].images[0].2.starts_with(b"<svg"));
        let found = discover_sheet_templates(&[dir.join("..")]);
        assert!(found.iter().any(|f| f.name() == "sheet-presets"));
        assert!(!found.iter().any(|f| f.name() == "code-old"));
        // An old plugin (no such export): loads, none.
        if let Some(old) = built_plugin("tests/code-old") {
            assert!(host.sheet_templates(&old).unwrap().is_empty());
            assert!(host.load_code(&old).is_ok());
        }
        // A manifest naming a file outside its folder is refused.
        let text = "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"wasm\"\nentry=\"t.wasm\"\n[[sheet-template]]\nfile = \"../x.ohlsheet\"";
        assert!(Manifest::parse(text, Path::new("/plugins/t")).is_err());
    }
}
