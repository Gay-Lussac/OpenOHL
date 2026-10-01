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

pub use bindings::ols::plugin::types::PluginInfo;

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

/// A parsed `ols-plugin.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Plugin identity and entry point.
    pub plugin: PluginSection,
    /// Requested permissions.
    #[serde(default)]
    pub permissions: Permissions,
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
        Ok(manifest)
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

/// Exit clauses applied to every plugin call.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum wall-clock time of one call (and of instantiation).
    pub call_timeout: Duration,
    /// Maximum linear memory of a plugin instance, in bytes.
    pub max_memory_bytes: usize,
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
    fn deadline_ticks(&self) -> u64 {
        (self.call_timeout.as_millis() / EPOCH_TICK.as_millis()).max(1) as u64
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

    /// Loads the plugin in `dir`: reads its manifest, checks compatibility
    /// and permissions, compiles and instantiates it.
    pub fn load(&self, dir: &Path) -> Result<WasmPlugin, PluginError> {
        let manifest = Manifest::load(dir)?;
        if manifest.plugin.tier != Tier::Wasm {
            return Err(PluginError::UnsupportedTier(
                manifest.plugin.tier.to_string(),
            ));
        }
        manifest.check_compatible()?;
        manifest.check_permissions()?;

        let component = Component::from_file(&self.engine, manifest.entry_path())?;
        // No stdio, environment, file system or network: a fully sandboxed guest.
        let state = HostState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(self.limits.max_memory_bytes)
                .build(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(self.limits.deadline_ticks());
        let name = manifest.plugin.name.clone();
        let instance = bindings::Plugin::instantiate(&mut store, &component, &self.linker)
            .map_err(|e| classify(e, &name, "instantiation", self.limits))?;
        Ok(WasmPlugin {
            manifest,
            store,
            instance,
            limits: self.limits,
        })
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
        assert!(elapsed >= Duration::from_millis(200), "{elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");

        // The host still works after the interruption.
        let mut other = host.load(&dir).unwrap();
        assert_eq!(other.describe().unwrap().name, "runaway");
    }
}
