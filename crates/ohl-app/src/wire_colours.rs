//! Line › Sections › Wire colours from a plugin… (owner, 2026-10-27): a
//! plugin's `wire-colours` export suggests a colour per circuit and phase
//! (e.g. a utility's colour table by owner, voltage, frequency and phase);
//! shown first, applied when asked as the sections' own colours, one
//! command (undone as one).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_line::Line;
use ohl_plugins::WireGroup;
use ohl_session::{Command, Kind};

use crate::app::Ready;

/// The sections of one circuit and phase (with their system).
#[derive(Debug, Clone)]
pub struct Group {
    /// What the plugin is sent.
    pub group: WireGroup,
    /// The sections' ids, in line order.
    pub sections: Vec<String>,
}

/// The line's sections grouped by circuit, phase and system, with what
/// `plugin`'s own columns hold on them (`plugin.column` on the system and
/// the sections; the section's value when both give one).
pub fn groups(line: &Line, plugin: &str) -> Vec<Group> {
    let prefix = format!("{plugin}.");
    let mut out: BTreeMap<(String, String, Option<String>), Group> = BTreeMap::new();
    for s in &line.sections {
        let key = (
            s.circuit_name.clone(),
            s.phase_label.clone(),
            s.system.clone(),
        );
        let g = out.entry(key).or_insert_with(|| {
            let system = s
                .system
                .as_ref()
                .and_then(|id| line.systems.iter().find(|x| &x.id == id));
            let mut fields: BTreeMap<String, String> = BTreeMap::new();
            if let Some(x) = system {
                for (k, v) in &x.fields {
                    if let Some(c) = k.strip_prefix(&prefix) {
                        fields.insert(c.to_owned(), v.clone());
                    }
                }
            }
            Group {
                group: WireGroup {
                    circuit: s.circuit_name.clone(),
                    phase: s.phase_label.clone(),
                    system: s.system.clone(),
                    system_name: system.map_or(String::new(), |x| x.name.clone()),
                    owner: system.map_or(String::new(), |x| x.owner.clone()),
                    number: system.map_or(String::new(), |x| x.number.clone()),
                    voltage_kv: system.map(|x| x.voltage_kv),
                    frequency_hz: system.and_then(|x| x.frequency_hz),
                    dc: system.is_some_and(|x| x.kind == ohl_line::electrical::SystemKind::Dc),
                    fields: fields.into_iter().collect(),
                    sections: 0,
                },
                sections: Vec::new(),
            }
        });
        for (k, v) in &s.fields {
            if let Some(c) = k.strip_prefix(&prefix)
                && !v.is_empty()
            {
                match g.group.fields.iter_mut().find(|(n, _)| n == c) {
                    Some(f) => f.1 = v.clone(),
                    None => g.group.fields.push((c.to_owned(), v.clone())),
                }
            }
        }
        g.group.sections += 1;
        g.sections.push(s.id.clone());
    }
    out.into_values().collect()
}

/// The sections whose colour changes (section id, new colour), from the
/// groups and the colours suggested for them; `all`: every section,
/// else only those without a colour of their own.
pub fn changes(
    line: &Line,
    groups: &[Group],
    colours: &[Option<String>],
    all: bool,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (g, c) in groups.iter().zip(colours) {
        let Some(c) = c else { continue };
        for id in &g.sections {
            let Some(s) = line.sections.iter().find(|s| &s.id == id) else {
                continue;
            };
            let keep = !all && s.color.is_some();
            if !keep && s.color.as_deref() != Some(c.as_str()) {
                out.push((id.clone(), c.clone()));
            }
        }
    }
    out
}

/// The plugin's answer: a colour or none per group, or why there is none.
type Answer = Result<Vec<Option<String>>, String>;

/// The window's state.
#[derive(Default)]
pub struct WireColours {
    /// Shown.
    pub open: bool,
    /// The plugin chosen (its name).
    plugin: String,
    /// Every section (else only those without a colour of their own).
    all: bool,
    /// The plugin is being asked.
    job: Option<Receiver<Answer>>,
    /// The groups asked about, and the colours given.
    result: Option<(Vec<Group>, Answer)>,
    /// What was done.
    status: Option<(String, bool)>,
}

impl WireColours {
    /// Opens the window.
    pub fn show(&mut self) {
        *self = Self {
            open: true,
            ..Self::default()
        };
    }

    /// The plugin is being asked (tests).
    #[cfg(test)]
    pub fn asking(&self) -> bool {
        self.job.is_some()
    }
}

/// Asks the plugin in `dir` in the background.
fn ask(dir: PathBuf, groups: Vec<WireGroup>) -> Receiver<Answer> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = ohl_plugins::WasmHost::new()
            .and_then(|h| h.wire_colours(&dir, &groups))
            .map_err(|e| e.to_string());
        let _ = tx.send(r);
    });
    rx
}

/// The window (Line › Sections › Wire colours from a plugin…).
pub fn window(ctx: &egui::Context, r: &mut Ready, plugins: &crate::plugins::Plugins) {
    if !r.wire_colours.open {
        return;
    }
    let w = &mut r.wire_colours;
    if let Some(rx) = &w.job
        && let Ok(got) = rx.try_recv()
    {
        w.job = None;
        if let Some((_, c)) = &mut w.result {
            *c = got;
        }
    }
    if w.job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let dirs = plugins.wire_colour_dirs();
    if !dirs.iter().any(|(n, _)| *n == w.plugin)
        && let Some((n, _)) = dirs.first()
    {
        w.plugin = n.clone();
    }
    let editable = r.model.can_edit();
    let mut open = true;
    let mut apply: Option<Vec<(String, String)>> = None;
    egui::Window::new("Wire colours from a plugin")
        .id(egui::Id::new("wire-colours-from-plugin"))
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "A plugin suggests a colour for each circuit and phase (e.g. a utility's \
                     colour table by owner, voltage, frequency and phase). Nothing changes until \
                     Apply; then the sections get them as their own colours, one command.",
                )
                .small()
                .weak(),
            );
            if let Err(why) = &editable {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            if dirs.is_empty() {
                crate::menu::error_label(
                    ui,
                    "No plugin in use suggests wire colours (extension point wire-colours; \
                     File › Plugins › Plugin manager).",
                );
                return;
            }
            ui.horizontal(|ui| {
                ui.label("plugin");
                egui::ComboBox::from_id_salt("wire-colours-plugin")
                    .selected_text(w.plugin.as_str())
                    .show_ui(ui, |ui| {
                        for (n, _) in &dirs {
                            if ui.selectable_value(&mut w.plugin, n.clone(), n).changed() {
                                w.result = None;
                            }
                        }
                    });
                ui.radio_value(&mut w.all, false, "sections without a colour");
                ui.radio_value(&mut w.all, true, "every section");
            });
            ui.horizontal(|ui| {
                let asking = w.job.is_some();
                if ui
                    .add_enabled(!asking, egui::Button::new("Ask the plugin"))
                    .clicked()
                    && let Some((_, dir)) = dirs.iter().find(|(n, _)| *n == w.plugin)
                {
                    let g = groups(&r.model.line, &w.plugin);
                    let sent: Vec<WireGroup> = g.iter().map(|x| x.group.clone()).collect();
                    w.job = Some(ask(dir.clone(), sent));
                    w.result = Some((g, Ok(Vec::new())));
                    w.status = None;
                }
                if asking {
                    ui.spinner();
                    ui.label("asking…");
                }
            });
            let Some((g, c)) = &w.result else {
                return;
            };
            if w.job.is_some() {
                return;
            }
            let colours = match c {
                Ok(c) => c,
                Err(e) => {
                    crate::menu::error_label(ui, e);
                    return;
                }
            };
            let todo = changes(&r.model.line, g, colours, w.all);
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("wire-colours-grid")
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["circuit", "phase", "system", "sections", "colour"] {
                                ui.label(egui::RichText::new(h).strong());
                            }
                            ui.end_row();
                            for (x, c) in g.iter().zip(colours) {
                                ui.label(&x.group.circuit);
                                ui.label(&x.group.phase);
                                ui.label(if x.group.system.is_none() {
                                    "earthed".to_owned()
                                } else {
                                    x.group.system_name.clone()
                                });
                                ui.label(x.group.sections.to_string());
                                match c
                                    .as_deref()
                                    .and_then(ohl_core::text::parse_rgb)
                                    .map(crate::color_tool::color32)
                                {
                                    Some(col) => {
                                        ui.horizontal(|ui| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(28.0, 12.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(rect, 2.0, col);
                                            ui.label(c.as_deref().unwrap_or_default());
                                        });
                                    }
                                    None => {
                                        ui.label(egui::RichText::new("none").weak());
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
            ui.separator();
            ui.label(format!(
                "{} section{} change colour",
                todo.len(),
                if todo.len() == 1 { "" } else { "s" }
            ));
            if ui
                .add_enabled(
                    editable.is_ok() && !todo.is_empty(),
                    egui::Button::new("Apply"),
                )
                .clicked()
            {
                apply = Some(todo);
            }
            if let Some((s, bad)) = &w.status {
                if *bad {
                    crate::menu::error_label(ui, s);
                } else {
                    ui.label(s);
                }
            }
        });
    if !open {
        r.wire_colours.open = false;
    }
    if let Some(todo) = apply {
        let plugin = r.wire_colours.plugin.clone();
        let mut commands = Vec::new();
        for (id, c) in &todo {
            let Some(s) = r.model.line.sections.iter().find(|s| &s.id == id) else {
                continue;
            };
            let mut s = s.clone();
            s.color = Some(c.clone());
            match serde_json::to_value(&s) {
                Ok(value) => commands.push(Command::Put {
                    kind: Kind::Section,
                    id: id.clone(),
                    value,
                    ord: None,
                }),
                Err(e) => {
                    r.wire_colours.status = Some((e.to_string(), true));
                    return;
                }
            }
        }
        let n = commands.len();
        let done = r.perform(crate::edit::Action::Command {
            what: format!("wire colours from {plugin} ({n} sections)"),
            command: Command::Batch { commands },
        });
        r.wire_colours.status = Some(if done {
            (format!("{n} sections coloured"), false)
        } else {
            ("not changed (see the status bar)".into(), true)
        });
        r.wire_colours.result = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::tests::{root, synthetic_project};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// The sections grouped by circuit, phase and system, with the
    /// system's owner and the plugin's own columns (the section's value
    /// over the system's); only the colours asked for change.
    #[test]
    fn sections_are_grouped_by_circuit_and_phase() {
        let mut line = synthetic_project("wire-colour-groups").line.clone();
        line.systems[0].owner = "Utility A".into();
        line.systems[0]
            .fields
            .insert("wire-colours.tint".into(), "dark".into());
        line.sections[1]
            .fields
            .insert("wire-colours.tint".into(), "pale".into());
        line.sections[1]
            .fields
            .insert("other.tint".into(), "x".into());
        let g = groups(&line, "wire-colours");
        let keys: Vec<(&str, &str, Option<&str>, &str)> = g
            .iter()
            .map(|x| {
                (
                    x.group.circuit.as_str(),
                    x.group.phase.as_str(),
                    x.group.system.as_deref(),
                    x.group.owner.as_str(),
                )
            })
            .collect();
        assert_eq!(
            keys,
            [
                ("Branch", "L1", None, ""),
                ("Example 1", "L1", Some("ex220"), "Utility A"),
                ("Example 1", "L2", Some("ex220"), "Utility A"),
            ]
        );
        assert_eq!(g[1].group.voltage_kv, Some(220.0));
        assert_eq!(g[1].group.fields, [("tint".to_owned(), "dark".to_owned())]);
        assert_eq!(g[2].group.fields, [("tint".to_owned(), "pale".to_owned())]);
        // Only those without a colour, or every section.
        line.sections[0].color = Some("#123456".into());
        let colours = [Some("#ff8000".into()), Some("#ff0000".into()), None];
        let c = changes(&line, &g, &colours, false);
        assert_eq!(c, [(line.sections[2].id.clone(), "#ff8000".to_owned())]);
        let c = changes(&line, &g, &colours, true);
        assert_eq!(c.len(), 2);
    }

    /// The window asks the test plugin (`plugins/tests/wire-colours`,
    /// when built) and applies its colours as one command.
    #[test]
    fn a_plugin_colours_the_wires() {
        let dir = root().join("plugins/tests/wire-colours");
        let built = ohl_plugins::Manifest::load(&dir)
            .unwrap()
            .entry_path()
            .is_file();
        assert!(
            built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "wire-colours test plugin not built"
        );
        if !built {
            eprintln!("skipping: plugins/tests/wire-colours not built");
            return;
        }
        let model = synthetic_project("wire-colours");
        let mut app = App::with_model(model, None);
        app.plugins = crate::plugins::Plugins::load(None, None);
        app.plugins.folders = vec![dir];
        app.plugins.refresh();
        app.ready().unwrap().wire_colours.show();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| app);
        harness.run();
        harness.get_by_label("Ask the plugin").click();
        harness.step();
        let started = std::time::Instant::now();
        while harness.state_mut().ready().unwrap().wire_colours.asking() {
            harness.step();
            assert!(started.elapsed().as_secs() < 60, "no answer in 60 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run();
        // The earthed branch: orange; the system's phases (owner not the
        // test table's): none.
        harness.get_by_label("#ff8000");
        harness.get_by_label("1 section change colour");
        harness.get_by_label("Apply").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        let colour = |r: &mut Ready, c: &str| {
            r.model
                .line
                .sections
                .iter()
                .find(|s| s.circuit_name == c)
                .and_then(|s| s.color.clone())
        };
        assert_eq!(colour(r, "Branch").as_deref(), Some("#ff8000"));
        assert_eq!(colour(r, "Example 1"), None);
        r.perform(crate::edit::Action::Undo);
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(colour(r, "Branch"), None);
    }
}
