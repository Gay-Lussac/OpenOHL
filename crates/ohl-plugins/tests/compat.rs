// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plugin interface keeps its promises (ADR-0012): what a released
//! version holds stays as it was, and plugins built for it still load and
//! answer. Compared with the frozen copies in `wit-frozen/`, and with the
//! plugin binaries in `tests/compat-0.3/` (built by
//! `tools/plugin_fixtures.sh`, kept as they were built).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wit_parser::{Resolve, Type, TypeDefKind, WorldItem};

/// A type written out in full: its name (when it has one) and structure.
fn ty(r: &Resolve, t: &Type) -> String {
    match t {
        Type::Bool => "bool".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::S8 => "s8".into(),
        Type::S16 => "s16".into(),
        Type::S32 => "s32".into(),
        Type::S64 => "s64".into(),
        Type::F32 => "f32".into(),
        Type::F64 => "f64".into(),
        Type::Char => "char".into(),
        Type::String => "string".into(),
        Type::ErrorContext => "error-context".into(),
        Type::Id(id) => {
            let def = &r.types[*id];
            let opt = |t: &Option<Type>| t.as_ref().map_or("_".into(), |t| ty(r, t));
            let body = match &def.kind {
                TypeDefKind::Record(x) => format!(
                    "record{{{}}}",
                    x.fields
                        .iter()
                        .map(|f| format!("{}:{}", f.name, ty(r, &f.ty)))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                TypeDefKind::Variant(x) => format!(
                    "variant{{{}}}",
                    x.cases
                        .iter()
                        .map(|c| format!("{}({})", c.name, opt(&c.ty)))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                TypeDefKind::Enum(x) => format!(
                    "enum{{{}}}",
                    x.cases
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                TypeDefKind::Flags(x) => format!(
                    "flags{{{}}}",
                    x.flags
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                TypeDefKind::Tuple(x) => format!(
                    "tuple<{}>",
                    x.types
                        .iter()
                        .map(|t| ty(r, t))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                TypeDefKind::Option(t) => format!("option<{}>", ty(r, t)),
                TypeDefKind::Result(x) => format!("result<{},{}>", opt(&x.ok), opt(&x.err)),
                TypeDefKind::List(t) => format!("list<{}>", ty(r, t)),
                TypeDefKind::Type(t) => ty(r, t),
                other => format!("{other:?}"),
            };
            match &def.name {
                Some(n) => format!("{n}={body}"),
                None => body,
            }
        }
    }
}

/// A function's signature in full.
fn func(r: &Resolve, f: &wit_parser::Function) -> String {
    format!(
        "({}) -> {}",
        f.params
            .iter()
            .map(|p| format!("{}:{}", p.name, ty(r, &p.ty)))
            .collect::<Vec<_>>()
            .join(","),
        f.result.as_ref().map_or("()".into(), |t| ty(r, t))
    )
}

/// What a world holds: imports and exports by name, each written out.
type World = (BTreeMap<String, String>, BTreeMap<String, String>);

/// An interface: its types and functions, each written out.
type Interface = (BTreeMap<String, String>, BTreeMap<String, String>);

/// The interfaces and worlds of the `ohl:plugin` package in `dir`.
fn read(dir: &Path) -> (BTreeMap<String, Interface>, BTreeMap<String, World>) {
    let mut r = Resolve::default();
    let (pkg, _) = r.push_dir(dir).expect("the interface parses");
    let item = |r: &Resolve, i: &WorldItem| match i {
        WorldItem::Interface { id, .. } => {
            format!("interface {}", r.id_of(*id).unwrap_or_default())
        }
        WorldItem::Function(f) => format!("func {}", func(r, f)),
        WorldItem::Type { id, .. } => format!("type {}", ty(r, &Type::Id(*id))),
    };
    let mut interfaces = BTreeMap::new();
    for (name, id) in &r.packages[pkg].interfaces {
        let i = &r.interfaces[*id];
        let types = i
            .types
            .iter()
            .map(|(n, t)| (n.clone(), ty(&r, &Type::Id(*t))))
            .collect();
        let funcs = i
            .functions
            .iter()
            .map(|(n, f)| (n.clone(), func(&r, f)))
            .collect();
        interfaces.insert(name.clone(), (types, funcs));
    }
    let mut worlds = BTreeMap::new();
    for (name, id) in &r.packages[pkg].worlds {
        let w = &r.worlds[*id];
        let side = |m: &wit_parser::IndexMap<wit_parser::WorldKey, WorldItem>| {
            m.iter()
                .map(|(k, i)| (r.name_world_key(k), item(&r, i)))
                .collect::<BTreeMap<_, _>>()
        };
        worlds.insert(name.clone(), (side(&w.imports), side(&w.exports)));
    }
    (interfaces, worlds)
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every released interface is still in the live one: each interface keeps
/// every type and function as it was (new ones may be added), each world
/// every import and exactly its exports (a new export would be asked of
/// plugins built without it; add it as a new interface or world instead).
#[test]
fn the_interface_keeps_every_released_item() {
    let (live_i, live_w) = read(&root().join("wit"));
    let mut frozen: Vec<PathBuf> = std::fs::read_dir(root().join("wit-frozen"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    frozen.sort();
    assert!(!frozen.is_empty());
    let mut broken = Vec::new();
    for dir in frozen {
        let version = dir.file_name().unwrap().to_string_lossy().into_owned();
        let (old_i, old_w) = read(&dir);
        for (name, (types, funcs)) in &old_i {
            let Some((now_t, now_f)) = live_i.get(name) else {
                broken.push(format!("{version}: interface {name} removed"));
                continue;
            };
            for (k, v) in types {
                if now_t.get(k) != Some(v) {
                    broken.push(format!(
                        "{version}: {name}: type {k} was {v}, is {:?}",
                        now_t.get(k)
                    ));
                }
            }
            for (k, v) in funcs {
                if now_f.get(k) != Some(v) {
                    broken.push(format!(
                        "{version}: {name}: function {k} was {v}, is {:?}",
                        now_f.get(k)
                    ));
                }
            }
        }
        for (name, (imports, exports)) in &old_w {
            let Some((now_i, now_e)) = live_w.get(name) else {
                broken.push(format!("{version}: world {name} removed"));
                continue;
            };
            for (k, v) in imports {
                if now_i.get(k) != Some(v) {
                    broken.push(format!("{version}: world {name}: import {k} changed"));
                }
            }
            if now_e != exports {
                broken.push(format!(
                    "{version}: world {name}: exports were {:?}, are {:?}",
                    exports.keys().collect::<Vec<_>>(),
                    now_e.keys().collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// The guard works: an interface with a field added to a released record,
/// or an export added to a released world, is caught.
#[test]
fn a_breaking_change_is_caught() {
    let dir = std::env::temp_dir().join(format!("ohl-compat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let plugin = std::fs::read_to_string(root().join("wit/plugin.wit")).unwrap();
    let code = std::fs::read_to_string(root().join("wit/code.wit")).unwrap();
    std::fs::write(dir.join("code.wit"), &code).unwrap();
    let field = plugin.replacen(
        "record plugin-info {",
        "record plugin-info {\n        added: u32,",
        1,
    );
    assert_ne!(field, plugin);
    std::fs::write(dir.join("plugin.wit"), &field).unwrap();
    let (old_i, _) = read(&root().join("wit-frozen/0.3"));
    let (now_i, _) = read(&dir);
    assert_ne!(old_i["types"], now_i["types"]);
    let export = plugin.replacen(
        "export span-load:",
        "export added: func() -> u32;\n    export span-load:",
        1,
    );
    std::fs::write(dir.join("plugin.wit"), &export).unwrap();
    let (_, old_w) = read(&root().join("wit-frozen/0.3"));
    let (_, now_w) = read(&dir);
    assert_ne!(old_w["plugin"].1, now_w["plugin"].1);
    std::fs::remove_dir_all(&dir).ok();
}

/// Plugins built for interface 0.3 (one of each world, kept as built)
/// load and answer: `ohl plugin check` passes on each.
#[test]
fn plugins_built_for_0_3_still_load() {
    let host = ohl_plugins::WasmHost::new().unwrap();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root().join("tests/compat-0.3"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("ohl-plugin.toml").is_file())
        .collect();
    dirs.sort();
    assert!(dirs.len() >= 11, "{dirs:?}");
    let mut failed = Vec::new();
    for dir in &dirs {
        let report = host.check_plugin(dir);
        if !report.passed() {
            failed.push(format!("{}: {:?}", dir.display(), report.lines));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    // A few answers, as the plugins gave them when built.
    let mut hello = host
        .load(&root().join("tests/compat-0.3/hello-wasm"))
        .unwrap();
    assert_eq!(hello.span_load(12.5, 300.0).unwrap(), 3750.0);
    let mut io = host
        .load_io(&root().join("tests/compat-0.3/geojson"))
        .unwrap();
    assert!(
        io.formats()
            .unwrap()
            .iter()
            .any(|f| f.extension == "geojson")
    );
}
