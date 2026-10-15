// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plugin manifests, compatibility checks and the WebAssembly component host.
//!
//! Every plugin ships an `ols-plugin.toml` manifest (ADR-0003):
//!
//! ```toml
//! [plugin]
//! name = "hello-wasm"
//! version = "0.1.0"
//! description = "Example plugin"
//! sdk = "0.1"                 # plugin interface version it targets
//! tier = "wasm"               # "wasm" or "python"
//! entry = "hello_wasm.wasm"   # relative to the manifest
//! extension-points = []
//!
//! [permissions]               # everything denied unless requested
//! network = false
//! filesystem = []
//! ```
//!
//! WebAssembly plugins implement the `ols:plugin` WIT world in `wit/` and run
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
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};
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

pub use terrain_bindings::ols::plugin::terrain_types::Surface;

mod project_bindings {
    wasmtime::component::bindgen!({
        world: "project-plugin",
        path: "wit",
    });
}

pub use project_bindings::ols::plugin::project_types::{LibraryFile, Project};

mod code_bindings {
    wasmtime::component::bindgen!({
        world: "code-plugin",
        path: "wit",
    });
}

pub use code_bindings::ols::plugin::code_types::{CodeInfo, Generated};

mod check_bindings {
    wasmtime::component::bindgen!({
        world: "check-plugin",
        path: "wit",
    });
}

pub use check_bindings::exports::ols::plugin::check::{
    CheckInfo, Choice, ConductorPair, Evaluated, ValueTable,
};

/// Extension point of check plugins (`check-plugin` world).
pub const CHECK: &str = "check";

/// Extension point of design-code plugins (`code-plugin` world).
pub const DESIGN_CODE: &str = "design-code";

pub use bindings::ols::plugin::types::PluginInfo;
pub use io_bindings::ols::plugin::io_types::{Direction, Format};

/// Version (major, minor) of the plugin interface implemented by this host.
pub const SDK_VERSION: (u64, u64) = (0, 1);

/// File name of plugin manifests.
pub const MANIFEST_FILE: &str = "ols-plugin.toml";

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
                "plugin {plugin} targets plugin interface {requested}, host supports {}.{}",
                SDK_VERSION.0, SDK_VERSION.1
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

/// A parsed `ols-plugin.toml`.
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
    /// Directory containing the manifest.
    #[serde(skip)]
    pub dir: PathBuf,
}

impl Manifest {
    /// Parses a manifest from text. `dir` is the directory it belongs to.
    pub fn parse(text: &str, dir: &Path) -> Result<Self, PluginError> {
        let mut manifest: Self = toml::from_str(text).map_err(|e| PluginError::Manifest {
            path: dir.join(MANIFEST_FILE),
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
                path: dir.join(MANIFEST_FILE),
                message: format!(
                    "entry {}: a path inside the plugin's folder",
                    manifest.plugin.entry.display()
                ),
            });
        }
        let bad = |message: String| PluginError::Manifest {
            path: dir.join(MANIFEST_FILE),
            message,
        };
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

    /// Loads `ols-plugin.toml` from a plugin directory.
    pub fn load(dir: &Path) -> Result<Self, PluginError> {
        let path = dir.join(MANIFEST_FILE);
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
}

/// A plugin directory found by [`discover`]: its manifest, or why it does
/// not read.
#[derive(Debug, Clone)]
pub struct Found {
    /// The plugin's directory (holding `ols-plugin.toml`).
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
        if let Err(e) = m.check_permissions() {
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
    if root.join(MANIFEST_FILE).is_file() {
        return vec![found(root)];
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST_FILE).is_file())
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
            std::fs::copy(from.join(MANIFEST_FILE), to.join(MANIFEST_FILE)).map_err(io)?;
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

    /// These limits raised to what `manifest` asks for (never lowered;
    /// capped).
    pub fn for_manifest(self, manifest: &Manifest) -> Self {
        let mut l = self;
        if let Some(s) = manifest.limits.seconds {
            l.call_timeout = l
                .call_timeout
                .max(Duration::from_secs(s).min(Self::MAX_TIMEOUT));
        }
        if let Some(m) = manifest.limits.memory_mib {
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
                .name("ols-plugin-epoch".into())
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
    limits: StoreLimits,
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
                path: dir.join(MANIFEST_FILE),
                message: format!("{kind} lists the extension point \"{point}\""),
            });
        }
        let limits = self.limits.for_manifest(&manifest);
        let component = Component::from_file(&self.engine, manifest.entry_path())?;
        // No stdio, environment, file system or network: a fully sandboxed guest.
        let state = HostState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.max_memory_bytes)
                .build(),
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
    pub fn load_code(&self, dir: &Path) -> Result<WasmCodePlugin, PluginError> {
        let mut p = self.prepare(dir, Some((DESIGN_CODE, "a design-code plugin")))?;
        let instance =
            code_bindings::CodePlugin::instantiate(&mut p.store, &p.component, &self.linker)
                .map_err(|e| classify(e, &p.manifest.plugin.name, "instantiation", p.limits))?;
        Ok(WasmCodePlugin {
            manifest: p.manifest,
            store: p.store,
            instance,
            limits: p.limits,
        })
    }
}

/// A loaded design-code plugin.
pub struct WasmCodePlugin {
    manifest: Manifest,
    store: Store<HostState>,
    instance: code_bindings::CodePlugin,
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
        self.instance
            .call_describe(&mut self.store)
            .map_err(|e| classify(e, &name, "describe", limits))
    }

    /// Calls `generate` with a parameter file's text (inner `Err`: the
    /// plugin's own error, about the file).
    pub fn generate(&mut self, parameters: &str) -> Result<Result<Generated, String>, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = self.manifest.plugin.name.clone();
        let limits = self.limits;
        self.instance
            .call_generate(&mut self.store, parameters)
            .map_err(|e| classify(e, &name, "generate", limits))
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
            limits: StoreLimitsBuilder::new()
                .memory_size(plugin.limits.max_memory_bytes)
                .build(),
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
            &check_bindings::exports::ols::plugin::check::Guest,
            &mut Store<HostState>,
        ) -> wasmtime::Result<T>,
    ) -> Result<T, PluginError> {
        self.store.set_epoch_deadline(self.limits.deadline_ticks());
        let (name, limits) = (self.manifest.plugin.name.clone(), self.limits);
        f(self.instance.ols_plugin_check(), &mut self.store)
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
                    terrain_bindings::ols::plugin::io_types::Direction::Exporter => {
                        Direction::Exporter
                    }
                    terrain_bindings::ols::plugin::io_types::Direction::Importer => {
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
        let m = manifest("0.1", "extension-points = [\"checks\"]");
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
            "0.1",
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
                "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.1\"\ntier=\"wasm\"\nentry=\"t.wasm\"\n[[column]]\n{bad}"
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
                "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.1\"\ntier=\"wasm\"\nentry=\"{entry}\""
            );
            assert!(
                Manifest::parse(&text, Path::new("/plugins/t")).is_err(),
                "{entry}"
            );
        }
        let text =
            "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.1\"\ntier=\"wasm\"\nentry=\"bin/t.wasm\"";
        assert!(Manifest::parse(text, Path::new("/plugins/t")).is_ok());
    }

    #[test]
    fn rejects_unknown_fields() {
        let text = "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.1\"\ntier=\"wasm\"\nentry=\"t.wasm\"\nunknown=1";
        assert!(matches!(
            Manifest::parse(text, Path::new(".")),
            Err(PluginError::Manifest { .. })
        ));
    }

    #[test]
    fn compatibility_rules() {
        assert!(manifest("0.1", "").check_compatible().is_ok());
        assert!(manifest("0.2", "").check_compatible().is_err());
        assert!(manifest("1.0", "").check_compatible().is_err());
        assert!(manifest("zero", "").check_compatible().is_err());
    }

    #[test]
    fn permissions_denied_by_default() {
        assert!(manifest("0.1", "").check_permissions().is_ok());
        let net = manifest("0.1", "[permissions]\nnetwork = true");
        assert!(matches!(
            net.check_permissions(),
            Err(PluginError::PermissionDenied { .. })
        ));
        let fs = manifest("0.1", "[permissions]\nfilesystem = [\"/tmp\"]");
        assert!(fs.check_permissions().is_err());
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
        let root = std::env::temp_dir().join(format!("ols-plugins-{}", std::process::id()));
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
            std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
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
}
