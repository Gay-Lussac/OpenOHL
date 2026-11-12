// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plugin SDK's tools (ADR-0012): the extension points and the worlds
//! they are loaded in, the interface's WIT files as this program carries
//! them, a new plugin's folder from a template (`ohl plugin new`) and the
//! check of a plugin folder before it is shared (`ohl plugin check`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{
    CHECK_WEATHER_EXPORT, FIELD_PRESETS_EXPORT, MEMBER_CAPACITY_EXPORT, Manifest, PluginError,
    RULE_PRESETS_EXPORT, SDK_VERSION, SHEET_TEMPLATES_EXPORT, STRUCTURE_WIND_EXPORT,
    SYSTEM_STYLES_EXPORT, SystemInfo, Tier, WIRE_COLOURS_EXPORT, WasmHost, WireGroup,
};

/// The interface's WIT files (file name, text), as this program was built
/// with them: what a plugin is built against.
pub const WIT_FILES: [(&str, &str); 2] = [
    ("plugin.wit", include_str!("../wit/plugin.wit")),
    ("code.wit", include_str!("../wit/code.wit")),
];

/// The version of `wit-bindgen` the templates use (the one the plugins
/// of the repository are built with).
pub const WIT_BINDGEN: &str = "0.62";

/// An extension point: what a plugin's manifest lists to be asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionPoint {
    /// Its name in `extension-points`.
    pub name: &'static str,
    /// The world a plugin of this point alone is built for.
    pub world: &'static str,
    /// Its export, when the point is an interface any plugin may add to
    /// its world (none: the point is the world's own functions).
    pub export: Option<&'static str>,
    /// What the application asks it for.
    pub what: &'static str,
}

/// Every extension point the applications ask plugins for.
pub const EXTENSION_POINTS: [ExtensionPoint; 11] = [
    ExtensionPoint {
        name: "design-code",
        world: "code-plugin",
        export: None,
        what: "weather and load cases from the user's parameter file",
    },
    ExtensionPoint {
        name: "check",
        world: "check-plugin",
        export: Some("ohl:plugin/check@0.1.0"),
        what: "checks of the line (Checks menu), with their value tables",
    },
    ExtensionPoint {
        name: "io",
        world: "io-plugin",
        export: None,
        what: "file formats to export the line to or import it from",
    },
    ExtensionPoint {
        name: "terrain",
        world: "terrain-plugin",
        export: None,
        what: "terrain surfaces read from a file",
    },
    ExtensionPoint {
        name: "project",
        world: "project-plugin",
        export: None,
        what: "whole projects read from another program's files",
    },
    ExtensionPoint {
        name: "sheet-templates",
        world: "sheet-template-plugin",
        export: Some(SHEET_TEMPLATES_EXPORT),
        what: "sheet templates for the drawings (or files named in the manifest)",
    },
    ExtensionPoint {
        name: "wire-colours",
        world: "wire-colour-plugin",
        export: Some(WIRE_COLOURS_EXPORT),
        what: "a colour per wire group",
    },
    ExtensionPoint {
        name: "system-styles",
        world: "system-style-plugin",
        export: Some(SYSTEM_STYLES_EXPORT),
        what: "a colour, line codes and phase angles per electrical system",
    },
    ExtensionPoint {
        name: "member-capacity",
        world: "member-capacity-plugin",
        export: Some(MEMBER_CAPACITY_EXPORT),
        what: "member capacities of structures by a design method",
    },
    ExtensionPoint {
        name: "member-check",
        world: "member-check-plugin",
        export: Some(crate::MEMBER_CHECK_EXPORT),
        what: "the member check of structures by a design method (every member and load case in one call)",
    },
    ExtensionPoint {
        name: "field-presets",
        world: "field-preset-plugin",
        export: Some(FIELD_PRESETS_EXPORT),
        what: "cuts and places for the electric and magnetic fields, and their reports",
    },
];

/// The extension point named `name`.
pub fn extension_point(name: &str) -> Option<&'static ExtensionPoint> {
    EXTENSION_POINTS.iter().find(|p| p.name == name)
}

/// The kinds of plugin `ohl plugin new` writes: an extension point's name,
/// or `minimal` (the `plugin` world, no extension point).
pub fn template_kinds() -> Vec<&'static str> {
    let mut out = vec!["minimal"];
    out.extend(EXTENSION_POINTS.iter().map(|p| p.name));
    out
}

/// Writes the interface's WIT files into `dir` (created), replacing those
/// there: the files a plugin is built against. Returns their paths.
pub fn write_wit(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut out = Vec::new();
    for (name, text) in WIT_FILES {
        let path = dir.join(name);
        std::fs::write(&path, text)?;
        out.push(path);
    }
    Ok(out)
}

/// A plugin name `ohl plugin new` takes: lower-case letters, digits and
/// `-`, starting with a letter (the manifest's name, the crate's and, with
/// `_`, the WebAssembly file's).
pub fn check_name(name: &str) -> Result<(), String> {
    let ok = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "plugin name \"{name}\": lower-case letters, digits and -, starting with a letter"
        ))
    }
}

/// Writes a new plugin of kind `kind` ([`template_kinds`]) named `name`
/// into `dir` (created; it must be empty or new): its manifest, its
/// Cargo.toml, the WIT files, a skeleton that builds and passes
/// `ohl plugin check`, a README and a .gitignore. Returns the files
/// written.
pub fn new_plugin(dir: &Path, name: &str, kind: &str) -> Result<Vec<PathBuf>, String> {
    check_name(name)?;
    let (world, point, source) = template_source(kind)
        .ok_or_else(|| format!("kind \"{kind}\": one of {}", template_kinds().join(", ")))?;
    if dir.exists()
        && std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .next()
            .is_some()
    {
        return Err(format!("{} is not empty: give a new folder", dir.display()));
    }
    let crate_file = name.replace('-', "_");
    let points = match point {
        Some(p) => format!("[\"{p}\"]"),
        None => "[]".to_owned(),
    };
    let manifest = format!(
        r#"[plugin]
name = "{name}"
version = "0.1.0"
description = "{description}"
sdk = "{major}.{minor}"            # the plugin interface version it is built for
tier = "wasm"
entry = "target/wasm32-wasip2/release/{crate_file}.wasm"
extension-points = {points}

[permissions]             # everything denied by default
network = false
filesystem = []

# Its own help pages, in this folder (Markdown or HTML), opened by the
# applications' "user guide" buttons; all optional:
# [help]
# overview = "docs/index.md"
# [help.check]            # per check id; also preset, column,
# my-check = "docs/my-check.md"   # sheet-template, class-scheme; design-code = "…"
"#,
        description = describe_kind(kind),
        major = SDK_VERSION.0,
        minor = SDK_VERSION.1,
    );
    let cargo = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

# Built on its own, for the wasm32-wasip2 target.
[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "{WIT_BINDGEN}"

[profile.release]
opt-level = "s"
lto = true
strip = true
"#
    );
    let readme = format!(
        "# {name}\n\n\
         An OpenOHL plugin ({world} world, made by `ohl plugin new`).\n\n\
         Build and check it:\n\n\
         ```bash\n\
         rustup target add wasm32-wasip2   # once\n\
         cargo build --release --target wasm32-wasip2\n\
         ohl plugin check .\n\
         ```\n\n\
         `wit/` holds the plugin interface it is built against (`ohl plugin wit wit`\n\
         writes the program's own). Install it in OHL-Design: Plugins › Manage plugins ›\n\
         Add folder, or copy this folder into the plugin folder.\n"
    );
    std::fs::create_dir_all(dir.join("src")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written = Vec::new();
    for (file, text) in [
        ("ohl-plugin.toml", manifest.as_str()),
        ("Cargo.toml", cargo.as_str()),
        ("README.md", readme.as_str()),
        (".gitignore", "/target\n"),
        ("src/lib.rs", source.as_str()),
    ] {
        let path = dir.join(file);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
    }
    written.extend(write_wit(&dir.join("wit")).map_err(|e| format!("wit: {e}"))?);
    Ok(written)
}

/// The description a new plugin of `kind` starts with.
fn describe_kind(kind: &str) -> String {
    match extension_point(kind) {
        Some(p) => format!("A new plugin: {}", p.what),
        None => "A new plugin".to_owned(),
    }
}

/// The licence header and the bindings of a template's source.
fn head(world: &str, what: &str) -> String {
    format!(
        "//! An OpenOHL plugin ({world} world): {what}.\n\
         //!\n\
         //! Made by `ohl plugin new`: fill in the functions below. The\n\
         //! interface is in `wit/` (see the comments there for every field).\n\
         \n\
         wit_bindgen::generate!({{\n    world: \"{world}\",\n    path: \"wit\",\n}});\n\
         \n\
         struct Plugin;\n\
         \n"
    )
}

/// `describe` of the worlds that have one with `plugin-info`.
fn describe_fn() -> &'static str {
    "    fn describe() -> PluginInfo {
        PluginInfo {
            name: env!(\"CARGO_PKG_NAME\").into(),
            version: env!(\"CARGO_PKG_VERSION\").into(),
            description: \"A new plugin\".into(),
        }
    }
"
}

/// The world, extension point and Rust source of a new plugin of `kind`.
fn template_source(kind: &str) -> Option<(&'static str, Option<&'static str>, String)> {
    let (world, point, what, body): (&str, Option<&str>, &str, String) = match kind {
        "minimal" => (
            "plugin",
            None,
            "a plugin of no extension point, to start from",
            format!(
                "impl Guest for Plugin {{\n{}\n    fn span_load(load_per_meter: f64, span_length: f64) -> f64 {{\n        load_per_meter * span_length\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "design-code" => (
            "code-plugin",
            Some("design-code"),
            "weather and load cases from a parameter file",
            "/// The parameter file the user starts from (TOML): every value of the\n\
             /// design code is the user's.\n\
             const TEMPLATE: &str = \"# Parameters of the design code (fill in)\\n\";\n\
             \n\
             impl Guest for Plugin {\n    fn describe() -> CodeInfo {\n        CodeInfo {\n            name: env!(\"CARGO_PKG_NAME\").into(),\n            version: env!(\"CARGO_PKG_VERSION\").into(),\n            description: \"A new plugin\".into(),\n            parameters_template: TEMPLATE.into(),\n        }\n    }\n\n    fn generate(parameters: String) -> Result<Generated, String> {\n        let _ = parameters;\n        Ok(Generated {\n            weather_cases: String::new(),\n            load_cases: String::new(),\n            notes: vec![\"nothing generated yet\".into()],\n            clearance_presets: Vec::new(),\n        })\n    }\n}\n"
                .to_owned(),
        ),
        "check" => (
            "check-plugin",
            Some("check"),
            "checks of the line",
            "use exports::ohl::plugin::check::{CheckInfo, ConductorPair, Evaluated, ValueTable};\n\
             \n\
             impl exports::ohl::plugin::check::Guest for Plugin {\n    fn checks() -> Vec<CheckInfo> {\n        vec![CheckInfo {\n            id: \"example\".into(),\n            title: \"Example check\".into(),\n            menu: vec![\"Example\".into()],\n            explanation: \"What the check does (fill in)\".into(),\n            values: String::new(),\n            values_extension: \"toml\".into(),\n            choices: Vec::new(),\n            circuit_columns: Vec::new(),\n        }]\n    }\n\n    fn value_tables(check: String, values: String) -> Result<Vec<ValueTable>, String> {\n        let _ = (check, values);\n        Ok(Vec::new())\n    }\n\n    fn edit_value(\n        check: String,\n        values: String,\n        table: u32,\n        row: u32,\n        column: u32,\n        text: String,\n    ) -> Result<String, String> {\n        let _ = (check, table, row, column, text);\n        Ok(values)\n    }\n\n    fn evaluate(\n        check: String,\n        values: String,\n        choices: Vec<u32>,\n        pair: ConductorPair,\n        stations: Vec<f64>,\n    ) -> Result<Evaluated, String> {\n        let _ = (check, values, choices, pair);\n        Ok(Evaluated {\n            values: vec![0.0; stations.len()],\n            per_station: 1,\n            rule: \"nothing required yet\".into(),\n        })\n    }\n}\n"
                .to_owned(),
        ),
        "io" => (
            "io-plugin",
            Some("io"),
            "a file format",
            format!(
                "use ohl::plugin::io_types::Direction;\n\
                 \n\
                 impl Guest for Plugin {{\n{}\n    fn formats() -> Vec<Format> {{\n        vec![Format {{\n            id: \"example\".into(),\n            description: \"Example format (fill in)\".into(),\n            extension: \"txt\".into(),\n            direction: Direction::Exporter,\n        }}]\n    }}\n\n    fn export_model(\n        format: String,\n        model: String,\n        options: Vec<(String, String)>,\n    ) -> Result<Vec<u8>, String> {{\n        let _ = (format, options);\n        Ok(model.into_bytes())\n    }}\n\n    fn import_file(\n        format: String,\n        data: Vec<u8>,\n        options: Vec<(String, String)>,\n    ) -> Result<String, String> {{\n        let _ = (data, options);\n        Err(format!(\"{{format}}: no importer yet\"))\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "terrain" => (
            "terrain-plugin",
            Some("terrain"),
            "terrain surfaces read from a file",
            format!(
                "use ohl::plugin::io_types::Direction;\n\
                 \n\
                 impl Guest for Plugin {{\n{}\n    fn formats() -> Vec<Format> {{\n        vec![Format {{\n            id: \"example\".into(),\n            description: \"Example surface format (fill in)\".into(),\n            extension: \"txt\".into(),\n            direction: Direction::Importer,\n        }}]\n    }}\n\n    fn import_surfaces(\n        format: String,\n        data: Vec<u8>,\n        options: Vec<(String, String)>,\n    ) -> Result<Vec<Surface>, String> {{\n        let _ = (data, options);\n        Err(format!(\"{{format}}: not read yet\"))\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "project" => (
            "project-plugin",
            Some("project"),
            "whole projects read from another program's files",
            format!(
                "use ohl::plugin::io_types::Direction;\n\
                 \n\
                 impl Guest for Plugin {{\n{}\n    fn formats() -> Vec<Format> {{\n        vec![Format {{\n            id: \"example\".into(),\n            description: \"Example project format (fill in)\".into(),\n            extension: \"txt\".into(),\n            direction: Direction::Importer,\n        }}]\n    }}\n\n    fn import_project(\n        format: String,\n        data: Vec<u8>,\n        options: Vec<(String, String)>,\n    ) -> Result<Project, String> {{\n        let _ = (data, options);\n        Err(format!(\"{{format}}: not read yet\"))\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "sheet-templates" => (
            "sheet-template-plugin",
            Some("sheet-templates"),
            "sheet templates for the drawings",
            format!(
                "use exports::ohl::plugin::sheet_templates::SheetTemplate;\n\
                 \n\
                 impl Guest for Plugin {{\n{}}}\n\
                 \n\
                 impl exports::ohl::plugin::sheet_templates::Guest for Plugin {{\n    fn sheet_templates() -> Result<Vec<SheetTemplate>, String> {{\n        Ok(Vec::new())\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "wire-colours" => (
            "wire-colour-plugin",
            Some("wire-colours"),
            "a colour per wire group",
            format!(
                "use exports::ohl::plugin::wire_colours::WireGroup;\n\
                 \n\
                 impl Guest for Plugin {{\n{}}}\n\
                 \n\
                 impl exports::ohl::plugin::wire_colours::Guest for Plugin {{\n    /// A colour (\"#rrggbb\") per group, in order; none: no suggestion.\n    fn wire_colours(groups: Vec<WireGroup>) -> Result<Vec<Option<String>>, String> {{\n        Ok(vec![None; groups.len()])\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "system-styles" => (
            "system-style-plugin",
            Some("system-styles"),
            "a style per electrical system",
            format!(
                "use exports::ohl::plugin::system_styles::{{SystemInfo, SystemStyle}};\n\
                 \n\
                 impl Guest for Plugin {{\n{}}}\n\
                 \n\
                 impl exports::ohl::plugin::system_styles::Guest for Plugin {{\n    /// A style per system, in order; none and no phases: no suggestion.\n    fn system_styles(systems: Vec<SystemInfo>) -> Result<Vec<SystemStyle>, String> {{\n        Ok(systems\n            .iter()\n            .map(|_| SystemStyle {{\n                color: None,\n                phases: Vec::new(),\n            }})\n            .collect())\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "member-check" => (
            "member-check-plugin",
            Some("member-check"),
            "the member check of structures by a design method",
            format!(
                "use exports::ohl::plugin::member_check::{{CheckInput, MemberResult, MethodInfo}};\n\
                 \n\
                 impl Guest for Plugin {{\n{}}}\n\
                 \n\
                 impl exports::ohl::plugin::member_check::Guest for Plugin {{\n    fn methods() -> Vec<MethodInfo> {{\n        vec![MethodInfo {{\n            name: \"example\".into(),\n            description: \"Example method (fill in)\".into(),\n        }}]\n    }}\n\n    // Every checked member of the structure with its forces in every\n    // load case, in this one call. A member left out of the result is\n    // reported as NOT CHECKED, NOT ON THE SAFE SIDE.\n    fn check_members(method: String, input: CheckInput) -> Result<Vec<MemberResult>, String> {{\n        let _ = input;\n        Err(format!(\"method {{method}}: not written yet\"))\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "member-capacity" => (
            "member-capacity-plugin",
            Some("member-capacity"),
            "member capacities by a design method",
            format!(
                "use exports::ohl::plugin::member_capacity::{{MemberInput, MemberResult, MethodInfo}};\n\
                 \n\
                 impl Guest for Plugin {{\n{}}}\n\
                 \n\
                 impl exports::ohl::plugin::member_capacity::Guest for Plugin {{\n    fn methods() -> Vec<MethodInfo> {{\n        vec![MethodInfo {{\n            name: \"example\".into(),\n            description: \"Example method (fill in)\".into(),\n        }}]\n    }}\n\n    fn member_capacities(\n        method: String,\n        strength: f64,\n        members: Vec<MemberInput>,\n    ) -> Result<Vec<MemberResult>, String> {{\n        let _ = strength;\n        Err(format!(\n            \"method {{method}}: not written yet ({{}} members asked)\",\n            members.len()\n        ))\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        "field-presets" => (
            "field-preset-plugin",
            Some("field-presets"),
            "cuts and places for the fields, and their reports",
            format!(
                "use exports::ohl::plugin::field_presets::{{CutReport, FieldInput, FieldPreset, OutFile}};\n\
                 \n\
                 impl Guest for Plugin {{\n{}}}\n\
                 \n\
                 impl exports::ohl::plugin::field_presets::Guest for Plugin {{\n    fn parameters_template() -> String {{\n        \"# Parameters of the field presets (fill in)\\n\".into()\n    }}\n\n    fn field_presets(input: FieldInput) -> Result<Vec<FieldPreset>, String> {{\n        let _ = input;\n        Ok(Vec::new())\n    }}\n\n    fn report_cut(\n        parameters: String,\n        plan: String,\n        preset: FieldPreset,\n        cut: String,\n    ) -> Result<CutReport, String> {{\n        let _ = (parameters, plan, cut);\n        Err(format!(\"{{}}: no report yet\", preset.name))\n    }}\n\n    fn report(\n        parameters: String,\n        plan: String,\n        preset: FieldPreset,\n        results: String,\n        cuts: Vec<String>,\n    ) -> Result<Vec<OutFile>, String> {{\n        let _ = (parameters, plan, preset, results, cuts);\n        Ok(Vec::new())\n    }}\n}}\n",
                describe_fn()
            ),
        ),
        _ => return None,
    };
    Some((
        world,
        point,
        format!("{}{body}\nexport!(Plugin);\n", head(world, what)),
    ))
}

/// How a line of a plugin check reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// It is as it should be.
    Ok,
    /// Worth knowing, nothing to fix.
    Note,
    /// Works, but probably not as meant.
    Warning,
    /// Does not work: the applications will not use it so.
    Error,
}

/// What `ohl plugin check` found, one line each.
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    /// The lines, in the order checked.
    pub lines: Vec<(Level, String)>,
}

impl CheckReport {
    fn push(&mut self, level: Level, text: impl Into<String>) {
        self.lines.push((level, text.into()));
    }

    /// The lines of `level`.
    pub fn count(&self, level: Level) -> usize {
        self.lines.iter().filter(|(l, _)| *l == level).count()
    }

    /// Whether nothing is in error.
    pub fn passed(&self) -> bool {
        self.count(Level::Error) == 0
    }
}

/// An invented wire group and system the check hands the plugin.
fn sample_group() -> WireGroup {
    WireGroup {
        circuit: "C1".into(),
        phase: "L1".into(),
        system: Some("S1".into()),
        system_name: "System 1".into(),
        owner: "Owner".into(),
        number: "1".into(),
        voltage_kv: Some(110.0),
        frequency_hz: Some(50.0),
        dc: false,
        fields: Vec::new(),
        sections: 1,
    }
}

fn sample_system() -> SystemInfo {
    SystemInfo {
        id: "S1".into(),
        name: "System 1".into(),
        owner: "Owner".into(),
        number: "1".into(),
        voltage_kv: 110.0,
        frequency_hz: Some(50.0),
        dc: false,
        phases: vec!["L1".into(), "L2".into(), "L3".into()],
        fields: Vec::new(),
        sections: 1,
    }
}

impl WasmHost {
    /// Checks the plugin in `dir` as the applications will use it: its
    /// manifest (interface version, permissions, extension points), its
    /// entry, then — a WebAssembly plugin — every extension point it lists
    /// loaded and called once with invented input, within the limits it is
    /// given, and the optional exports it has against those it lists.
    pub fn check_plugin(&self, dir: &Path) -> CheckReport {
        let mut r = CheckReport::default();
        let manifest = match Manifest::load(dir) {
            Ok(m) => m,
            Err(e) => {
                r.push(Level::Error, e.to_string());
                return r;
            }
        };
        let p = &manifest.plugin;
        r.push(
            Level::Ok,
            format!("manifest: {} {} ({} tier)", p.name, p.version, p.tier),
        );
        match manifest.check_compatible() {
            Ok(()) => r.push(
                Level::Ok,
                format!(
                    "interface version {} (this program: {}.{})",
                    p.sdk, SDK_VERSION.0, SDK_VERSION.1
                ),
            ),
            Err(e) => r.push(Level::Error, e.to_string()),
        }
        match manifest.check_permissions() {
            Ok(()) => r.push(Level::Ok, "permissions: none asked beyond the sandbox"),
            Err(e) => r.push(Level::Error, e.to_string()),
        }
        for e in &p.extension_points {
            if extension_point(e).is_none() {
                r.push(
                    Level::Warning,
                    format!(
                        "extension point \"{e}\" is not one of {}: no application asks for it",
                        EXTENSION_POINTS.map(|p| p.name).join(", ")
                    ),
                );
            }
        }
        // Its help pages (`[help]`): each must be in its folder.
        let pages = manifest.help.pages().len();
        let missing = manifest.help.missing(&manifest.dir);
        if pages > 0 && missing.is_empty() {
            r.push(Level::Ok, format!("help pages: {pages} found"));
        }
        for m in missing {
            r.push(Level::Error, format!("help page {m}"));
        }
        let entry = manifest.entry_path();
        if !entry.is_file() {
            let hint = if p.tier == Tier::Wasm {
                " (build it: cargo build --release --target wasm32-wasip2)"
            } else {
                ""
            };
            r.push(
                Level::Error,
                format!("entry {} not found{hint}", entry.display()),
            );
            return r;
        }
        if p.tier != Tier::Wasm {
            r.push(
                Level::Note,
                format!(
                    "entry {}: a {} plugin is run by the application, not loaded here",
                    entry.display(),
                    p.tier
                ),
            );
            return r;
        }
        if !r.passed() {
            return r;
        }
        let size = std::fs::metadata(&entry).map_or(0, |m| m.len());
        let started = Instant::now();
        match self.describe_any(dir) {
            Ok(d) => {
                r.push(
                    Level::Ok,
                    format!(
                        "loads as {} ({} kB, {:.0} ms)",
                        d.worlds.join(", "),
                        size / 1024,
                        started.elapsed().as_secs_f64() * 1e3
                    ),
                );
                if !d.worlds.contains(&"check-plugin")
                    && (d.info.name != p.name || d.info.version != p.version)
                {
                    r.push(
                        Level::Warning,
                        format!(
                            "describe gives {} {}, the manifest {} {}: the same is expected",
                            d.info.name, d.info.version, p.name, p.version
                        ),
                    );
                }
                for line in d.details {
                    r.push(Level::Note, line);
                }
            }
            Err(e) => {
                r.push(Level::Error, e.to_string());
                return r;
            }
        }
        self.check_exports(dir, &manifest, &mut r);
        self.check_points(dir, &manifest, &mut r);
        r
    }

    /// The optional exports the component has against the extension
    /// points its manifest lists.
    fn check_exports(&self, dir: &Path, manifest: &Manifest, r: &mut CheckReport) {
        let prepared = match self.prepare(dir, None) {
            Ok(p) => p,
            Err(e) => {
                r.push(Level::Error, e.to_string());
                return;
            }
        };
        let has = |name: &str| prepared.component.get_export_index(None, name).is_some();
        let lists = |p: &str| manifest.plugin.extension_points.iter().any(|e| e == p);
        for point in EXTENSION_POINTS {
            let Some(export) = point.export else {
                continue;
            };
            let files = point.name == "sheet-templates" && !manifest.sheet_templates.is_empty();
            match (has(export), lists(point.name)) {
                (true, false) => r.push(
                    Level::Warning,
                    format!(
                        "it exports {} but its manifest does not list the extension point \"{}\": \
                         the applications will not ask it",
                        point.name, point.name
                    ),
                ),
                (false, true) if !files => r.push(
                    Level::Error,
                    format!(
                        "its manifest lists \"{}\" but it does not export {}",
                        point.name, export
                    ),
                ),
                _ => {}
            }
        }
        let optional: Vec<&str> = [
            (STRUCTURE_WIND_EXPORT, "structure-wind"),
            (RULE_PRESETS_EXPORT, "rule-presets"),
            (CHECK_WEATHER_EXPORT, "check-weather"),
        ]
        .into_iter()
        .filter_map(|(e, n)| has(e).then_some(n))
        .collect();
        if !optional.is_empty() {
            r.push(Level::Note, format!("also exports {}", optional.join(", ")));
        }
    }

    /// Each extension point the manifest lists, called once.
    fn check_points(&self, dir: &Path, manifest: &Manifest, r: &mut CheckReport) {
        let lists = |p: &str| manifest.plugin.extension_points.iter().any(|e| e == p);
        let mut call = |what: &str, result: Result<String, PluginError>| match result {
            Ok(text) => r.push(Level::Ok, format!("{what}: {text}")),
            Err(e) => r.push(Level::Error, format!("{what}: {e}")),
        };
        if lists("design-code") {
            call(
                "design-code, its own parameter template",
                self.load_code(dir).and_then(|mut c| {
                    let template = c.describe()?.parameters_template;
                    Ok(match c.generate(&template)? {
                        Ok(g) => format!(
                            "generates {} line(s) of weather cases, {} of load cases, {} note(s)",
                            g.weather_cases.lines().count(),
                            g.load_cases.lines().count(),
                            g.notes.len()
                        ),
                        Err(e) => {
                            format!("refused with \"{e}\" (fine if the template needs filling in)")
                        }
                    })
                }),
            );
        }
        if lists("check") {
            call(
                "check",
                self.load_check(dir).and_then(|mut c| {
                    let checks = c.checks()?;
                    Ok(format!(
                        "{} check(s): {}",
                        checks.len(),
                        checks
                            .iter()
                            .map(|c| c.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }),
            );
        }
        if lists("sheet-templates") {
            call(
                "sheet-templates",
                self.sheet_templates(dir)
                    .map(|t| format!("{} template(s)", t.len())),
            );
        }
        if lists("wire-colours") {
            call(
                "wire-colours, one invented group",
                self.wire_colours(dir, &[sample_group()])
                    .map(|c| format!("{:?}", c[0])),
            );
        }
        if lists("system-styles") {
            call(
                "system-styles, one invented system",
                self.system_styles(dir, &[sample_system()]).map(|s| {
                    format!(
                        "colour {:?}, {} phase style(s)",
                        s[0].color,
                        s[0].phases.len()
                    )
                }),
            );
        }
        if lists("member-check") {
            call(
                "member-check",
                self.member_check_methods(dir).map(|m| {
                    format!(
                        "{} method(s): {}",
                        m.len(),
                        m.iter()
                            .map(|m| m.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }),
            );
        }
        if lists("member-capacity") {
            call(
                "member-capacity",
                self.member_capacity_methods(dir).map(|m| {
                    format!(
                        "{} method(s): {}",
                        m.len(),
                        m.iter()
                            .map(|m| m.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }),
            );
        }
        if !manifest.plan_sets.is_empty() {
            call(
                "plan sets",
                crate::plan_sets(dir).map(|s| format!("{} file(s) read", s.len())),
            );
        }
        if !manifest.class_schemes.is_empty() {
            call(
                "class schemes",
                crate::class_schemes(dir).and_then(|s| {
                    match s.iter().find(|(_, p)| !p.is_file()) {
                        Some((_, p)) => Err(PluginError::Manifest {
                            path: crate::manifest_path(dir),
                            message: format!("class scheme {} not found", p.display()),
                        }),
                        None => Ok(format!("{} file(s)", s.len())),
                    }
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every extension point is a kind of template, and its world is one
    /// the interface has.
    #[test]
    fn every_point_has_a_template_and_a_world() {
        let wit: String = WIT_FILES.iter().map(|(_, t)| *t).collect();
        for p in EXTENSION_POINTS {
            assert!(template_source(p.name).is_some(), "{}", p.name);
            assert!(
                wit.contains(&format!("world {} {{", p.world)),
                "{}",
                p.world
            );
        }
        assert!(template_source("minimal").is_some());
        assert!(template_source("nothing").is_none());
        assert_eq!(template_kinds().len(), EXTENSION_POINTS.len() + 1);
    }

    /// Names: lower-case letters, digits and -, from a letter.
    #[test]
    fn plugin_names() {
        assert!(check_name("my-code-2").is_ok());
        for bad in ["", "My", "2nd", "a_b", "a-", "a b"] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
    }

    /// A new plugin's folder: its files, a manifest the host reads (its
    /// extension point listed, this interface version), the WIT files; a
    /// folder that is not empty is refused.
    #[test]
    fn a_new_plugin_is_written() {
        let dir = std::env::temp_dir().join(format!("ohl-sdk-new-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let files = new_plugin(&dir, "my-colours", "wire-colours").unwrap();
        assert_eq!(files.len(), 7);
        let m = Manifest::load(&dir).unwrap();
        assert_eq!(m.plugin.name, "my-colours");
        assert_eq!(m.plugin.extension_points, ["wire-colours"]);
        assert!(m.check_compatible().is_ok());
        assert!(m.entry_path().ends_with("my_colours.wasm"));
        let source = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
        assert!(source.contains("world: \"wire-colour-plugin\""));
        assert_eq!(
            std::fs::read_to_string(dir.join("wit/plugin.wit")).unwrap(),
            WIT_FILES[0].1
        );
        assert!(new_plugin(&dir, "my-colours", "wire-colours").is_err());
        assert!(new_plugin(&dir.join("x"), "my-colours", "colours").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A folder without a manifest, and one whose entry is not built, are
    /// errors of the check, saying what to do.
    #[test]
    fn the_check_says_what_is_missing() {
        let host = WasmHost::new().unwrap();
        let dir = std::env::temp_dir().join(format!("ohl-sdk-check-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!host.check_plugin(&dir).passed());
        std::fs::remove_dir_all(&dir).ok();
        new_plugin(&dir, "unbuilt", "io").unwrap();
        let r = host.check_plugin(&dir);
        assert!(!r.passed());
        assert!(
            r.lines
                .iter()
                .any(|(l, t)| *l == Level::Error && t.contains("cargo build")),
            "{r:?}"
        );
        // A help page declared but not there: an error; there: found.
        let manifest = dir.join(crate::MANIFEST_FILE);
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            format!("{text}\n[help]\noverview = \"docs/index.md\"\n"),
        )
        .unwrap();
        let r = host.check_plugin(&dir);
        assert!(
            r.lines
                .iter()
                .any(|(l, t)| *l == Level::Error && t.contains("docs/index.md not found")),
            "{r:?}"
        );
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("docs/index.md"), "# x\n").unwrap();
        let r = host.check_plugin(&dir);
        assert!(
            r.lines
                .iter()
                .any(|(l, t)| *l == Level::Ok && t == "help pages: 1 found"),
            "{r:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
