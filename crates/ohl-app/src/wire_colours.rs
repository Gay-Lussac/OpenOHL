//! Line › Sections › Wire colours from a plugin… (owner, 2026-10-27): a
//! plugin's `wire-colours` export suggests a colour per circuit and phase
//! (e.g. a utility's colour table by owner, voltage, frequency and phase);
//! shown first, applied when asked as the sections' own colours; and its
//! `system-styles` export a colour per electrical system, a line code per
//! phase (to tell them apart when wires are drawn by system) and the
//! phase angles — all in one command (undone as one).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_line::Line;
use ohl_plugins::{SystemInfo, SystemStyle, WireGroup};
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

/// The line's electrical systems as the plugin is sent them, with what
/// `plugin`'s own columns hold on them and how many sections carry each.
pub fn system_infos(line: &Line, plugin: &str) -> Vec<SystemInfo> {
    let prefix = format!("{plugin}.");
    line.systems
        .iter()
        .map(|x| SystemInfo {
            id: x.id.clone(),
            name: x.name.clone(),
            owner: x.owner.clone(),
            number: x.number.clone(),
            voltage_kv: x.voltage_kv,
            frequency_hz: x.frequency_hz,
            dc: x.kind == ohl_line::electrical::SystemKind::Dc,
            phases: x.phases.iter().map(|p| p.label.clone()).collect(),
            fields: x
                .fields
                .iter()
                .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|c| (c.to_owned(), v.clone())))
                .collect(),
            sections: line
                .sections
                .iter()
                .filter(|s| s.system.as_deref() == Some(x.id.as_str()))
                .count() as u32,
        })
        .collect()
}

/// The line's systems with the styles suggested applied — the colours and
/// line codes when `styles` is ticked, the phase angles when `angles` is
/// — and how many systems change; `None` when none does.
pub fn styled_systems(
    line: &Line,
    suggested: &[SystemStyle],
    styles: bool,
    angles: bool,
) -> Option<(Vec<ohl_line::electrical::System>, usize)> {
    let mut systems = line.systems.clone();
    let mut changed = 0;
    for (x, st) in systems.iter_mut().zip(suggested) {
        let before = x.clone();
        if styles && let Some(c) = &st.color {
            x.color = Some(c.clone());
        }
        for (p, ps) in x.phases.iter_mut().zip(&st.phases) {
            if styles && let Some(code) = &ps.code {
                p.code = Some(code.clone());
            }
            if angles && let Some(a) = ps.angle_deg {
                p.angle_deg = Some(a);
            }
        }
        if *x != before {
            changed += 1;
        }
    }
    (changed > 0).then_some((systems, changed))
}

/// The plugin's answer: a colour or none per group and a style per
/// system, or why there is none.
type Answer = Result<(Vec<Option<String>>, Vec<SystemStyle>), String>;

/// What Apply writes: the sections' colours, the systems changed.
type Apply = (
    Vec<(String, String)>,
    Option<Vec<ohl_line::electrical::System>>,
);

/// The window's state.
#[derive(Default)]
pub struct WireColours {
    /// Shown.
    pub open: bool,
    /// The plugin chosen (its name).
    plugin: String,
    /// Every section (else only those without a colour of their own).
    all: bool,
    /// Apply the sections' colours.
    colours: bool,
    /// Apply the systems' colours and line codes.
    styles: bool,
    /// Apply the phase angles (off by default: they feed the electrical
    /// calculations).
    angles: bool,
    /// The plugin is being asked.
    job: Option<Receiver<Answer>>,
    /// The groups and systems asked about, and the answer.
    result: Option<(Vec<Group>, Vec<SystemInfo>, Answer)>,
    /// What was done.
    status: Option<(String, bool)>,
}

impl WireColours {
    /// Opens the window.
    pub fn show(&mut self) {
        *self = Self {
            open: true,
            colours: true,
            styles: true,
            ..Self::default()
        };
    }

    /// The plugin is being asked (tests).
    #[cfg(test)]
    pub fn asking(&self) -> bool {
        self.job.is_some()
    }
}

/// Asks the plugin in `dir` in the background: the wire colours and the
/// system styles (each none when it does not give them).
fn ask(dir: PathBuf, groups: Vec<WireGroup>, systems: Vec<SystemInfo>) -> Receiver<Answer> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = ohl_plugins::WasmHost::new()
            .and_then(|h| {
                Ok((
                    h.wire_colours(&dir, &groups)?,
                    h.system_styles(&dir, &systems)?,
                ))
            })
            .map_err(|e| e.to_string());
        let _ = tx.send(r);
    });
    rx
}

/// A colour swatch with its `#rrggbb`, or "none".
fn swatch(ui: &mut egui::Ui, c: Option<&str>) {
    match c
        .and_then(ohl_core::text::parse_rgb)
        .map(crate::color_tool::color32)
    {
        Some(col) => {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(28.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, col);
                ui.label(c.unwrap_or_default());
            });
        }
        None => {
            ui.label(egui::RichText::new("none").weak());
        }
    }
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
        if let Some((_, _, c)) = &mut w.result {
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
    let mut apply: Option<Apply> = None;
    egui::Window::new("Wire colours from a plugin")
        .id(egui::Id::new("wire-colours-from-plugin"))
        .open(&mut open)
        .default_width(600.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "A plugin suggests a colour for each circuit and phase, and for each \
                     electrical system its colour, the line codes that tell its phases apart \
                     when wires are drawn by system, and its phase angles (e.g. a utility's \
                     tables). Nothing changes until Apply; then, as ticked, one command.",
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
                    "No plugin in use suggests wire colours or system styles (extension \
                     points wire-colours, system-styles; File › Plugins › Plugin manager).",
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
                let asking = w.job.is_some();
                if ui
                    .add_enabled(!asking, egui::Button::new("Ask the plugin"))
                    .clicked()
                    && let Some((_, dir)) = dirs.iter().find(|(n, _)| *n == w.plugin)
                {
                    let g = groups(&r.model.line, &w.plugin);
                    let sys = system_infos(&r.model.line, &w.plugin);
                    let sent: Vec<WireGroup> = g.iter().map(|x| x.group.clone()).collect();
                    w.job = Some(ask(dir.clone(), sent, sys.clone()));
                    w.result = Some((g, sys, Ok((Vec::new(), Vec::new()))));
                    w.status = None;
                }
                if asking {
                    ui.spinner();
                    ui.label("asking…");
                }
            });
            let Some((g, sys, c)) = &w.result else {
                return;
            };
            if w.job.is_some() {
                return;
            }
            let (colours, styles) = match c {
                Ok(c) => c,
                Err(e) => {
                    crate::menu::error_label(ui, e);
                    return;
                }
            };
            let any_colour = colours.iter().any(Option::is_some);
            let any_style = styles
                .iter()
                .any(|s| s.color.is_some() || s.phases.iter().any(|p| p.code.is_some()));
            let any_angle = styles
                .iter()
                .any(|s| s.phases.iter().any(|p| p.angle_deg.is_some()));
            let todo = if w.colours {
                changes(&r.model.line, g, colours, w.all)
            } else {
                Vec::new()
            };
            let systems = styled_systems(&r.model.line, styles, w.styles, w.angles);
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() * 0.5).max(160.0))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if any_colour {
                        ui.label(egui::RichText::new("Wires (sections)").strong());
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
                                    swatch(ui, c.as_deref());
                                    ui.end_row();
                                }
                            });
                    }
                    if any_style || any_angle {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Electrical systems").strong());
                        egui::Grid::new("system-styles-grid")
                            .striped(true)
                            .show(ui, |ui| {
                                for h in ["system", "kV", "colour", "phases: line code, angle"] {
                                    ui.label(egui::RichText::new(h).strong());
                                }
                                ui.end_row();
                                for (x, st) in sys.iter().zip(styles) {
                                    ui.label(if x.name.is_empty() { &x.id } else { &x.name });
                                    ui.label(format!("{}", x.voltage_kv));
                                    swatch(ui, st.color.as_deref());
                                    let phases: Vec<String> = x
                                        .phases
                                        .iter()
                                        .zip(&st.phases)
                                        .map(|(l, p)| {
                                            format!(
                                                "{l} {} {}",
                                                p.code.as_deref().unwrap_or("·"),
                                                p.angle_deg
                                                    .map_or(String::new(), |a| format!("{a}°"))
                                            )
                                        })
                                        .collect();
                                    ui.label(if phases.is_empty() {
                                        "none".to_owned()
                                    } else {
                                        phases.join("   ")
                                    });
                                    ui.end_row();
                                }
                            });
                    }
                    if !any_colour && !any_style && !any_angle {
                        ui.label(
                            egui::RichText::new("The plugin suggests nothing for this line.")
                                .weak(),
                        );
                    }
                });
            ui.separator();
            if any_colour {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut w.colours, "the wires' colours, for");
                    ui.radio_value(&mut w.all, false, "sections without a colour");
                    ui.radio_value(&mut w.all, true, "every section");
                });
            }
            if any_style {
                ui.checkbox(&mut w.styles, "the systems' colours and line codes");
            }
            if any_angle {
                ui.checkbox(&mut w.angles, "the phase angles")
                    .on_hover_text("They feed the electrical calculations: off unless asked");
            }
            let n_systems = systems.as_ref().map_or(0, |s| s.1);
            ui.label(format!(
                "{} section{} change colour, {} system{} change",
                todo.len(),
                if todo.len() == 1 { "" } else { "s" },
                n_systems,
                if n_systems == 1 { "" } else { "s" }
            ));
            if ui
                .add_enabled(
                    editable.is_ok() && (!todo.is_empty() || n_systems > 0),
                    egui::Button::new("Apply"),
                )
                .clicked()
            {
                apply = Some((todo, systems.map(|s| s.0)));
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
    if let Some((todo, systems)) = apply {
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
        let m = systems.as_ref().map_or(0, Vec::len);
        if let Some(systems) = systems {
            let mut line = r.model.line.clone();
            line.systems = systems;
            commands.push(ohl_session::line_settings_change(&r.model.line, &line));
        }
        let done = r.perform(crate::edit::Action::Command {
            what: format!("wire colours and system styles from {plugin}"),
            command: Command::Batch { commands },
        });
        r.wire_colours.status = Some(if done {
            (
                format!(
                    "{n} sections coloured{}",
                    if m > 0 { ", the systems styled" } else { "" }
                ),
                false,
            )
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
        let mut model = synthetic_project("wire-colours");
        // The system the test table knows (owner "Utility A", 220 kV).
        let mut after = model.line.clone();
        after.systems[0].owner = "Utility A".into();
        // An angle the plugin would not suggest: kept unless ticked.
        after.systems[0].phases[1].angle_deg = Some(-115.0);
        model
            .apply(
                "owner",
                ohl_session::line_settings_change(&model.line, &after),
            )
            .unwrap();
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
        // The earthed branch: orange; the system's phases (owner the
        // test table's): red and green.
        harness.get_by_label("#ff8000");
        // The system: green below 300 kV; its line codes; the angles only
        // when ticked.
        assert!(harness.get_all_by_label("#00ff00").count() >= 2);
        // The earthed branch and the system's two phases.
        harness.get_by_label("3 sections change colour, 1 system change");
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
        assert!(colour(r, "Example 1").is_some());
        let sys = &r.model.line.systems[0];
        assert_eq!(sys.color.as_deref(), Some("#00ff00"));
        assert_eq!(sys.phases[1].code.as_deref(), Some(".-"));
        assert_eq!(sys.phases[1].angle_deg, Some(-115.0), "angles not ticked");
        r.perform(crate::edit::Action::Undo);
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(colour(r, "Branch"), None);
        assert_eq!(r.model.line.systems[0].color, None, "one command, undone");
    }
}
