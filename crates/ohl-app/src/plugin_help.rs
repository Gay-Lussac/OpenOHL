// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A plugin's own help pages (2026-11-12; [`ohl_plugins::help`]): the
//! *user guide* button of a window showing a plugin's check, preset or
//! design code opens the page its plugin declares for it (`[help]` in its
//! manifest); the Plugin manager's *Help* its overview and the others.
//! A Markdown page is rendered to HTML first; both open in the browser
//! ([`crate::help::open`]). No page declared: no button.

use std::path::Path;

use eframe::egui;
use ohl_plugins::HelpTopic;

/// The memory key of the last error opening the page of `topic` of
/// `dir`.
fn error_key(dir: &Path, topic: HelpTopic<'_>) -> egui::Id {
    egui::Id::new(("plugin-help-error", dir, format!("{topic:?}")))
}

/// Opens the page of `topic` the plugin in `dir` declares: what was
/// opened, or why not.
pub fn open(dir: &Path, topic: HelpTopic<'_>) -> Result<std::path::PathBuf, String> {
    let file = ohl_plugins::help::file_to_open(dir, topic)?;
    crate::help::open(&file)?;
    Ok(file)
}

/// The label of the button for `topic` of `plugin`: *user guide (its
/// name) ↗*, apart from the window's own *user guide ↗*; the overview
/// *Help ↗*.
pub fn label(plugin: &str, topic: HelpTopic<'_>) -> String {
    match topic {
        HelpTopic::Overview => "Help ↗".to_owned(),
        _ => format!("user guide ({plugin}) ↗"),
    }
}

/// *user guide (plugin) ↗* for `topic` of the plugin in `dir` (`plugin`:
/// its name), when the plugin declares a page for it (nothing drawn
/// otherwise); a click opens it, an error is shown beside it. True when
/// drawn.
pub fn button(ui: &mut egui::Ui, dir: &Path, plugin: &str, topic: HelpTopic<'_>) -> bool {
    button_labelled(ui, dir, plugin, topic, &label(plugin, topic))
}

/// [`button`] titled `text` (the distance windows, 2026-11-12: *the
/// plugin's rule (its name) ↗* beside OpenOHL's *how it is measured ↗*).
pub fn button_labelled(
    ui: &mut egui::Ui,
    dir: &Path,
    plugin: &str,
    topic: HelpTopic<'_>,
    text: &str,
) -> bool {
    if ohl_plugins::help::page_file(dir, topic).is_none() {
        return false;
    }
    let what = match topic {
        HelpTopic::Overview => "the plugin".to_owned(),
        HelpTopic::DesignCode => "its weather and load cases".to_owned(),
        HelpTopic::Check(id) => format!("its check {id}"),
        HelpTopic::Preset(n) => format!("its preset {n}"),
        HelpTopic::Column(n) => format!("its column {n}"),
        HelpTopic::SheetTemplate(n) => format!("its sheet template {n}"),
        HelpTopic::ClassScheme(n) => format!("its class scheme {n}"),
    };
    if ui
        .small_button(text)
        .on_hover_text(format!(
            "The plugin {plugin}'s own page on {what} (shipped with the plugin, opened in the \
             browser)"
        ))
        .clicked()
    {
        let r = open(dir, topic).err();
        ui.data_mut(|d| d.insert_temp(error_key(dir, topic), r));
    }
    if let Some(Some(e)) = ui.data(|d| d.get_temp::<Option<String>>(error_key(dir, topic))) {
        crate::menu::error_label(ui, &e);
    }
    true
}

/// The Plugin manager's help of the plugin in `dir`: *Help ↗* (its
/// overview) and, when it declares other pages, *pages…* listing them.
pub fn manager_buttons(ui: &mut egui::Ui, dir: &Path, plugin: &str) {
    let pages = ohl_plugins::help::pages_of(dir);
    if pages.is_empty() {
        return;
    }
    button(ui, dir, plugin, HelpTopic::Overview);
    let others: Vec<(String, std::path::PathBuf)> = pages
        .pages()
        .into_iter()
        .filter(|(what, _)| what != "overview")
        .map(|(w, p)| (w, p.to_path_buf()))
        .collect();
    if others.is_empty() {
        return;
    }
    ui.menu_button("pages…", |ui| {
        for (what, page) in others {
            let found = dir.join(&page).is_file();
            let r = ui
                .add_enabled(found, egui::Button::new(&what))
                .on_hover_text(page.display().to_string())
                .on_disabled_hover_text(format!("{} is not in its folder", page.display()));
            if r.clicked() {
                let e =
                    ohl_plugins::help::viewable(dir, &page, &ohl_plugins::help::default_cache())
                        .and_then(|f| crate::help::open(&f))
                        .err();
                ui.data_mut(|d| d.insert_temp(error_key(dir, HelpTopic::Overview), e));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    /// The button opens the page its plugin declares (Markdown rendered to
    /// HTML first); none declared: no button.
    #[test]
    fn the_button_opens_the_declared_page() {
        let root = std::env::temp_dir().join(format!("ohl-app-plugin-help-{}", std::process::id()));
        let dir = root.join("p");
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(
            dir.join("ohl-plugin.toml"),
            "[plugin]\nname = \"p\"\nversion = \"0.1.0\"\nsdk = \"0.3\"\ntier = \"wasm\"\n\
             entry = \"p.wasm\"\n[help.check]\nmy-check = \"docs/check.md\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("docs/check.md"), "# My check\n").unwrap();
        let opened = open(&dir, HelpTopic::Check("my-check")).unwrap();
        assert!(opened.ends_with("docs/check.html"));
        assert!(
            crate::help::OPENED.lock().unwrap().contains(&opened),
            "the rendered page opened"
        );
        assert!(open(&dir, HelpTopic::Check("other")).is_err());
        // Drawn only where a page is declared.
        let drawn = std::sync::Arc::new(std::sync::Mutex::new((false, false)));
        let (d, shown) = (dir.clone(), drawn.clone());
        let mut h = egui_kittest::Harness::new_ui(move |ui| {
            let a = button(ui, &d, "p", HelpTopic::Check("my-check"));
            let b = button(ui, &d, "p", HelpTopic::Overview);
            *shown.lock().unwrap() = (a, b);
        });
        h.run();
        assert_eq!(*drawn.lock().unwrap(), (true, false));
        h.get_by_label("user guide (p) ↗").click();
        h.run();
        assert!(
            crate::help::OPENED
                .lock()
                .unwrap()
                .iter()
                .filter(|p| **p == opened)
                .count()
                >= 2,
            "opened again by the click"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The public example plugin's overview is found and opens.
    #[test]
    fn the_example_plugin_page_is_found() {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../plugins/examples/hello-wasm"
        ));
        assert!(ohl_plugins::help::page_file(dir, HelpTopic::Overview).is_some());
        let f = open(dir, HelpTopic::Overview).unwrap();
        let html = std::fs::read_to_string(&f).unwrap();
        assert!(html.contains("<title>hello-wasm</title>"), "{html}");
        assert!(
            html.contains("span-load.svg"),
            "its drawing from its folder"
        );
    }
}
