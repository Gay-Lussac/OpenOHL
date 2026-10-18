// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OHL-Structure on its own (egui_kittest): the start page, a new
//! structure, File › Libraries and the Libraries panel, the plugin manager
//! with the settings shared with the line application, a design code's
//! load cases.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::StructureApp;
use crate::layout::Panel;
use crate::libraries::LibKind;
use crate::tests::{copy_of, menu};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn started(app: impl FnOnce(&mut StructureApp) + 'static) -> Harness<'static, StructureApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 900.0))
        .build_eframe(move |_| {
            let mut a = StructureApp::new(None);
            app(&mut a);
            a
        });
    h.run();
    h
}

/// Without a file: the start page with Open, New and the recent files; a
/// new structure (from the New structure window) is written and opened,
/// and is then the first recent file.
#[test]
fn the_start_page_opens_and_creates_a_structure() {
    let path = copy_of("type-01.ohls", "start");
    let dir = path.parent().unwrap().to_path_buf();
    let p = path.clone();
    let mut h = started(move |a| a.recent.add(&p));
    h.get_by_label("Open a structure…");
    // A recent file opens with a click.
    h.get_by_label("type-01.ohls").click();
    h.run();
    assert!(h.state().file.is_some());
    // Back on the start page: New structure… asks its id, name, source.
    h.state_mut().file = None;
    h.run();
    h.get_by_label("New structure…").click();
    h.run();
    h.get_by_label("New structure");
    h.get_by_label("Empty: one point (level 1), to build from");
    // Created where chosen (the dialog's answer given here).
    let new = dir.join("tower-a.ohls");
    {
        let a = h.state_mut();
        let mut n = a.new_structure.take().unwrap();
        n.id = "tower-a".into();
        n.name = "Tower A".into();
        a.create_structure(&new, &n).unwrap();
    }
    h.run();
    h.get_by_label("Level 1 — stick figure");
    assert_eq!(
        h.state().recent.files.first(),
        Some(&std::fs::canonicalize(&new).unwrap())
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// File › Libraries › Import a section library: listed in the Libraries
/// panel and the structure's sections resolve (the load panels read it);
/// what the structure names that no library has is listed; Export writes
/// the sections it uses.
#[test]
fn libraries_are_imported_listed_and_exported() {
    let path = copy_of("type-01.ohls", "libs");
    let dir = path.parent().unwrap().to_path_buf();
    // A member naming a library section, one naming none, an insulator.
    let text = std::fs::read_to_string(&path).unwrap().replacen(
        "wire = \"W1\"\nlength = \"5.025 m\"\nweight = \"0.0 N\"\n",
        "wire = \"W1\"\ninsulator = \"example-220-i\"\n",
        1,
    ) + "\n[[node]]\nid = \"F\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\n\n[[node]]\nid = \"T\"\n\
         point = [\"0 m\", \"0 m\", \"30 m\"]\n\n[[member]]\nid = \"M1\"\nnodes = [\"F\", \"T\"]\n\
         section = \"EA-61x7\"\n\n[[member]]\nid = \"M2\"\nnodes = [\"F\", \"T\"]\nsection = \
         \"NO-SUCH\"\n";
    std::fs::write(&path, text).unwrap();
    let p = path.clone();
    let mut h = started(move |a| a.open(&p));
    menu(&mut h, "View", "Libraries");
    h.run();
    assert!(h.state().is_open(Panel::Libraries));
    h.get_by_label("The structure names, but no library has:");
    h.get_by_label_contains("EA-61x7, NO-SUCH");
    // Imported (the file dialog's answer given here): read again.
    let lib = root().join("libraries/sections/example-sections.ohlsec");
    h.state_mut()
        .import_library(LibKind::Sections, &lib)
        .unwrap();
    h.state_mut()
        .import_library(LibKind::Insulators, &root().join("libraries/insulators"))
        .unwrap();
    h.run();
    assert!(dir.join("Sections/example-sections.ohlsec").is_file());
    assert!(h.state().loads.sections.section("EA-61x7").is_some());
    h.get_by_label("example-sections.ohlsec");
    h.get_by_label("example-suspension-i.ohli");
    h.get_by_label_contains("section(s) in no library: NO-SUCH");
    assert!(
        h.state_mut()
            .listing()
            .unwrap()
            .has(LibKind::Sections, "EA-61x7")
    );
    // A file that does not read: in a window, nothing imported.
    let typo = dir.join("typo.ohlsec");
    std::fs::write(
        &typo,
        "format = \"ohl-sections\"\nversion = 1\nnmae = \"x\"\n",
    )
    .unwrap();
    assert!(
        h.state_mut()
            .import_library(LibKind::Sections, &typo)
            .is_err()
    );
    h.run();
    h.get_by_label("Import Section libraries");
    h.get_by_label_contains("nmae");
    // Export what it uses.
    let out = dir.join("exported");
    h.state_mut()
        .export_libraries(crate::libraries::Which::Used, &out)
        .unwrap();
    let s = std::fs::read_to_string(out.join("sections/example-sections.ohlsec")).unwrap();
    assert!(s.contains("EA-61x7") && !s.contains("UA-83x52x6"), "{s}");
    assert!(out.join("structures/type-01.ohls").is_file());
    assert!(out.join("insulators/example-suspension-i.ohli").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

/// The plugin manager lists the in-repo example and code plugins (with
/// their extension points and what they do here); turning one off is
/// kept in the settings shared with the line application; a design code
/// generates load cases taken as recipes.
#[test]
fn plugins_are_managed_with_shared_settings() {
    let user = std::env::temp_dir().join(format!("ohl-structure-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user);
    let en50341 = root().join("plugins/codes/en50341");
    let built = en50341
        .join("target/wasm32-wasip2/release/en50341.wasm")
        .is_file();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "en50341 plugin not built"
    );
    // The line application added the folders (the shared settings).
    ohl_plugins::PluginSettings {
        folders: vec![
            root().join("plugins/examples"),
            root().join("plugins/codes"),
        ],
        disabled: Default::default(),
    }
    .save(&ohl_plugins::PluginSettings::path(&user))
    .unwrap();
    let path = copy_of("type-01.ohls", "plugins");
    let (u, p) = (user.clone(), path.clone());
    let mut h = started(move |a| {
        a.plugins = crate::plugins_ui::PluginsState::load(Some(u));
        a.open(&p);
    });
    menu(&mut h, "Plugins", "Plugin manager…");
    h.get_by_label("Plugin manager");
    h.get_by_label("hello-python");
    h.get_by_label("geojson");
    h.get_by_label("en50341");
    let has = |h: &Harness<'static, StructureApp>, text: &str| {
        h.query_all_by_label_contains(text).next().is_some()
    };
    assert!(has(&h, "load cases from a design code"));
    assert!(has(&h, "io — file formats of the line"));
    if built {
        // Turned off here: the shared file says so (the line application
        // reads it).
        h.get_by_label("en50341").click();
        h.run();
        let s = ohl_plugins::PluginSettings::load(&ohl_plugins::PluginSettings::path(&user))
            .unwrap()
            .unwrap();
        assert!(s.disabled.contains("en50341"), "{s:?}");
        assert!(h.state().plugins.codes().iter().all(|c| c.0 != "en50341"));
        h.get_by_label("en50341").click();
        h.run();
        assert!(h.state().plugins.codes().iter().any(|c| c.0 == "en50341"));
        // The design code: its load cases as recipes.
        {
            let a = h.state_mut();
            a.plugins.code = Some(crate::plugins_ui::CodeWindow {
                plugin: "en50341".into(),
                parameters: Some(en50341.join("example-parameters.toml")),
                ..Default::default()
            });
            a.plugins.start_code().unwrap();
            a.plugins.wait_code(std::time::Duration::from_secs(120));
        }
        h.run();
        let r = match &h.state().plugins.code.as_ref().unwrap().result {
            Some(Ok(r)) => r.clone(),
            other => panic!("{other:?}"),
        };
        assert!(r.names.len() >= 2, "{:?}", r.names);
        h.get_by_label("Take them as recipes").click();
        h.run();
        let text = h.state().file.as_ref().unwrap().text();
        assert!(
            text.contains(&format!("line_case = \"{}\"", r.names[0])),
            "{text}"
        );
        assert!(h.state().dirty());
    }
    std::fs::remove_dir_all(&user).ok();
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
