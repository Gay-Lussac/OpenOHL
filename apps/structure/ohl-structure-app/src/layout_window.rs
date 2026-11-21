// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Lay out joints** (2026-11-18, stage 4 of the automatic joint layout):
//! Design › Lay out joints… and Joint details › Lay out joint open one
//! tool window (the tool-window pattern): which joints (the joint shown,
//! the joints picked, a member group's, the whole tower; with their
//! similar and symmetric joints), the settings (the gap between steel,
//! the longest pull-back, the shift limit, the bolt where an end has none,
//! a connection preset per member class, the plates of a corner bolted or
//! welded, values given by hand replaced or kept), then a preview worked
//! out on a copy (on a thread of its own): what changes per joint, the
//! joints left for a hand detail with why, the clashes before and after,
//! the time it took, each joint's close-up before and after, and on
//! request the members' usage before and after for the last run's cases.
//! **Apply** writes the preview as one undo step; nothing is written
//! before. The layout follows later edits ([`StructureApp::follow_setup`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_link::{
    BoltArea, GAP_START, JointDetails, LayoutRecord, LayoutRun, LayoutSettings, PULL_BACK_START,
    RunAsk, SHIFT_LIMIT_START,
};

use crate::app::StructureApp;

/// The manual's section of the window (page, anchor).
pub const LAYOUT_PAGE: (&str, &str) = ("user-guide/joint-layout", LAYOUT_ANCHOR);
const LAYOUT_ANCHOR: &str = "lay-out-joints";

/// The member classes a preset may be given for.
pub const CLASSES: [&str; 4] = ["bracing", "redundant", "crossing", "leg"];

/// The diameter of the bolt picked when the libraries have none, mm (the
/// drawing's usual bolt; an assumption, changed in the window).
const BOLT_START_MM: f64 = 16.0;

/// Which joints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// The joint shown in Joint details.
    Joint(String),
    /// The joints picked.
    Picked,
    /// The joints of a member group.
    Group(String),
    /// Every joint.
    Tower,
}

/// The preview: a run on a copy, and the joints in detail before and
/// after.
pub struct Preview {
    /// The inputs it was worked out for.
    pub key: u64,
    /// The file's revision it was worked out on.
    pub revision: u64,
    /// The run.
    pub run: LayoutRun,
    /// The joints asked in detail before and after.
    pub before: JointDetails,
    /// After.
    pub after: JointDetails,
}

/// The members' usage before and after for the last run's cases.
pub struct UsageCompare {
    /// The preview's key it is of.
    pub key: u64,
    /// Member, its largest usage before and after.
    pub rows: Vec<(String, Option<f64>, Option<f64>)>,
    /// Cases solved.
    pub cases: usize,
    /// How long, s.
    pub seconds: f64,
}

pub(crate) type Job<T> = Option<(u64, Receiver<Result<T, String>>)>;

/// The Lay out joints window.
pub struct LayoutWindow {
    /// Which joints.
    pub scope: Scope,
    /// With their similar and symmetric joints.
    pub similar: bool,
    /// The gap between steel, mm.
    pub gap_mm: f64,
    /// The longest pull-back, mm.
    pub pull_mm: f64,
    /// The shift limit, mm.
    pub shift_mm: f64,
    /// The bolt picked (a library bolt).
    pub bolt: Option<String>,
    /// Its diameter, mm (typed when no library bolt).
    pub diameter_mm: f64,
    /// A preset per member class (by name).
    pub presets: BTreeMap<String, String>,
    /// The plates of a corner welded together (else each bolted to its
    /// flange).
    pub welded: bool,
    /// Values given by hand replaced too.
    pub replace: bool,
    /// The preview.
    pub preview: Option<Preview>,
    job: Job<Preview>,
    /// Why there is no preview.
    pub error: Option<String>,
    /// The joint whose close-ups are shown.
    pub shown: Option<String>,
    /// The two close-ups' views (before, after; turned together).
    pub views: Box<(
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    )>,
    /// The usage before and after.
    pub usage: Option<UsageCompare>,
    usage_job: Job<UsageCompare>,
}

impl LayoutWindow {
    /// The settings as the engine takes them.
    pub fn settings(&self, bolts: &ohl_structures::bolts::BoltLibrary) -> LayoutSettings {
        let lib = self.bolt.as_deref().and_then(|id| bolts.get(id));
        let d = lib.map_or(self.diameter_mm / 1000.0, |b| b.diameter.si());
        LayoutSettings {
            gap: self.gap_mm / 1000.0,
            pull_back: self.pull_mm / 1000.0,
            shift_limit: self.shift_mm / 1000.0,
            presets: BTreeMap::new(),
            bolt: Some(BoltArea::picked(
                self.bolt.clone(),
                d,
                lib.and_then(|b| b.drawn()[0]),
            )),
            welded: self.welded,
            // The structure file's default minimums, taken by the run.
            defaults: Default::default(),
            bent_plates: true,
            once_per_kind: true,
            // L1 (owner 2026-11-19): bottom up, then out; slightly more
            // plates; what an end bolts onto chosen in the search.
            bottom_up: true,
            more_plates: true,
            onto_search: true,
            // L2 (2026-11-20): plates on their member, clash-aware
            // planning, the tries before an end is left for a clash; L1 (c)
            // off (measured, left out).
            plate_on: true,
            exact_plan: true,
            clash_tries: true,
            direct_first: false,
            // L2's follow-ups and L3 (2026-11-20): a plate stopping short of
            // what it would run into, a bent plate's own bolts slid clear of
            // the holes there, a plate bent by a few degrees.
            short_plates: true,
            slide_holds: true,
            small_bends: true,
            // L3b (2026-11-20): an equal-leg angle's other leg tried for
            // the end bent the wrong way; an end running towards its bend
            // bent away from the flange too.
            bend_facing: true,
            bend_back: true,
            // L5 (2026-11-20): an end of a face going onto a plate bolted
            // directly where pulled back further, or moved by at most the
            // plate's thickness, its bolts land.
            direct_moved: true,
            // L5o (2026-11-20): a flat plate on a member running on through
            // the joint given two provisional bolts of its own onto it.
            hold_plates: true,
            // J6 (2026-11-20): a plate on the angle of a built-up member
            // facing the ends (its angles known), kept where it lays out
            // more.
            built_up_plates: true,
            // L8 (2026-11-21): a joint planned once more with the members
            // the run named but does not lay out where the file draws them.
            as_drawn: true,
            // L8 (2026-11-21): the bolts of a member left as it is that
            // bolts onto an end laid out kept on that end's steel.
            partner_bolts: true,
            // L9 (2026-11-21): a welded plate's flat plate planned with its
            // outline as written (else short, else the ends left); holding
            // bolts on a plate stopping short of the work point where they
            // fit.
            reach_clear: true,
            short_holds: true,
            // L10 (2026-11-21): one of two ends on a face of a leg's flange
            // tried on its other face (half a turn), a diagonal onto the
            // inner face first, kept where it lays out more or as many on
            // fewer plates and its leg lies no farther out than now.
            other_face: true,
            face_pattern: true,
            // L10 (2026-11-21): a joint with an end's bolts drawn that join
            // nothing is not laid out whole (that member by hand, said).
            nothing_joined: true,
        }
    }

    fn key(&self, revision: u64, libs: u64, targets: &BTreeSet<String>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        revision.hash(&mut h);
        libs.hash(&mut h);
        targets.hash(&mut h);
        for v in [self.gap_mm, self.pull_mm, self.shift_mm, self.diameter_mm] {
            v.to_bits().hash(&mut h);
        }
        (&self.bolt, &self.presets, self.welded, self.replace).hash(&mut h);
        h.finish()
    }
}

impl StructureApp {
    /// Opens the Lay out joints window for `scope`, its settings those of
    /// the last run (the file's record), else the starting values.
    pub(crate) fn open_layout_window(&mut self, scope: Scope) {
        self.layout_window = self.new_layout_window(scope);
    }

    /// The Lay out joints window for `scope` as it opens (the settings of
    /// the last run, else the starting values); also the settings Solve
    /// the clashes runs the layout with.
    pub(crate) fn new_layout_window(&mut self, scope: Scope) -> Option<LayoutWindow> {
        let f = self.file.as_ref()?;
        let rec = LayoutRecord::of(f.structure()).settings;
        let bolts = self.bolt_library();
        let first = bolts
            .0
            .bolts
            .iter()
            .find(|b| (b.diameter.si() * 1000.0 - BOLT_START_MM).abs() < 0.5)
            .or(bolts.0.bolts.first())
            .map(|b| b.id.clone());
        let (bolt, d) = match rec.as_ref().and_then(|r| r.bolt.clone()) {
            Some((id, d)) => (id, d * 1000.0),
            None => (first, BOLT_START_MM),
        };
        Some(LayoutWindow {
            scope,
            similar: true,
            gap_mm: rec.as_ref().map_or(GAP_START, |r| r.gap) * 1000.0,
            pull_mm: rec.as_ref().map_or(PULL_BACK_START, |r| r.pull_back) * 1000.0,
            shift_mm: rec.as_ref().map_or(SHIFT_LIMIT_START, |r| r.shift_limit) * 1000.0,
            bolt,
            diameter_mm: d,
            presets: rec.as_ref().map(|r| r.presets.clone()).unwrap_or_default(),
            welded: rec.as_ref().is_some_and(|r| r.welded),
            replace: false,
            preview: None,
            job: None,
            error: None,
            shown: None,
            views: {
                // A little further out than Joint details: the two views are small.
                let mut v: Box<(
                    crate::joint_panel::JointPanel,
                    crate::joint_panel::JointPanel,
                )> = Box::default();
                v.0.zoom = -0.4;
                v.1.zoom = -0.4;
                v
            },
            usage: None,
            usage_job: None,
        })
    }

    /// The layout's settings as the Lay out joints window would run them
    /// now (the last run's, else the starting values), the presets' fields
    /// by class and a leg splice's provisional bolt.
    pub(crate) fn layout_settings_now(
        &mut self,
    ) -> Option<(
        LayoutSettings,
        BTreeMap<String, toml_edit::Table>,
        Option<BoltArea>,
    )> {
        let w = self.new_layout_window(Scope::Tower)?;
        let bolts = self.bolt_library();
        let presets = self.preset_list();
        let mut s = w.settings(&bolts.0);
        let (areas, fields) = ohl_structure_link::presets_by_class(&w.presets, &presets, &bolts.0);
        let leg = areas.get("leg").cloned().or_else(|| s.bolt.clone());
        s.presets = areas;
        Some((s, fields, leg))
    }

    /// The joints the window's scope names (with the similar and
    /// symmetric ones when ticked).
    fn layout_targets(&mut self, w: &LayoutWindow) -> Option<BTreeSet<String>> {
        let f = self.file.as_ref()?;
        let t = f.structure_arc();
        let mut out: BTreeSet<String> = match &w.scope {
            Scope::Joint(j) => BTreeSet::from([j.clone()]),
            Scope::Picked => self
                .selected
                .iter()
                .filter_map(|h| match h {
                    Handle::Joint(j) => Some(j.clone()),
                    _ => None,
                })
                .collect(),
            Scope::Group(g) => t
                .members
                .iter()
                .filter(|m| m.group.as_deref() == Some(g.as_str()))
                .flat_map(|m| m.nodes.iter().cloned())
                .collect(),
            Scope::Tower => t.nodes.iter().map(|n| n.id.clone()).collect(),
        };
        if w.similar && w.scope != Scope::Tower {
            let details = self.joint_details();
            let mut more = BTreeSet::new();
            for j in &out {
                more.extend(ohl_structure_edit::symmetry::partners(&t, j));
                if let Some(Ok(d)) = details.as_deref() {
                    more.extend(
                        ohl_structure_link::similar_joints(d, j)
                            .into_iter()
                            .map(|s| s.joint),
                    );
                }
            }
            out.extend(more);
        }
        Some(out)
    }

    /// The presets as (name, fields).
    fn preset_list(&mut self) -> Vec<(String, toml_edit::Table)> {
        self.presets()
            .into_iter()
            .map(|p| (p.name, p.fields))
            .collect()
    }

    /// The Lay out joints window ([`Self::open_layout_window`]); its
    /// section of the manual: [`LAYOUT_PAGE`].
    pub(crate) fn layout_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.layout_window.take() else {
            return;
        };
        let Some(targets) = self.layout_targets(&w) else {
            return;
        };
        let bolts = self.bolt_library();
        let presets = self.preset_list();
        let (revision, text) = match self.file.as_ref() {
            Some(f) => (f.revision(), f.text()),
            None => return,
        };
        let key = w.key(revision, self.libs_stamp, &targets);
        // The preview: worked out again on its own thread when what it is
        // of changes.
        if let Some((k, rx)) = &w.job {
            match rx.try_recv() {
                Ok(r) => {
                    if *k == key {
                        match r {
                            Ok(p) => {
                                w.preview = Some(p);
                                w.error = None;
                            }
                            Err(e) => w.error = Some(e),
                        }
                    }
                    w.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(_) => w.job = None,
            }
        }
        let fresh = w.preview.as_ref().is_some_and(|p| p.key == key);
        if !fresh && w.job.as_ref().is_none_or(|(k, _)| *k != key) && !targets.is_empty() {
            let mut s = w.settings(&bolts.0);
            let (areas, fields) =
                ohl_structure_link::presets_by_class(&w.presets, &presets, &bolts.0);
            s.presets = areas;
            let lib = self.loads.sections.clone();
            let bolts2 = bolts.clone();
            let replace = w.replace;
            let joints = targets.clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let ask = RunAsk {
                    library: &lib,
                    bolts: &bolts2.0,
                    settings: &s,
                    preset_fields: fields,
                    joints: Some(joints.clone()),
                    replace_hand: replace,
                };
                let r = (|| {
                    let run = ohl_structure_link::lay_out(&text, &ask)?;
                    let before = ohl_structure_link::joint_details_at(
                        &text,
                        &lib,
                        &bolts2.0,
                        Some(&joints),
                    )?;
                    let after = ohl_structure_link::joint_details_at(
                        &run.text,
                        &lib,
                        &bolts2.0,
                        Some(&joints),
                    )?;
                    Ok(Preview {
                        key,
                        revision,
                        run,
                        before,
                        after,
                    })
                })();
                let _ = tx.send(r);
            });
            w.job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.poll_usage(&mut w, ctx);
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mut usage_asked = false;
        let mut in_line: Option<Vec<ohl_structure_link::InLine>> = None;
        let mut leg_places: Option<LegPlaces> = None;
        let groups: Vec<String> = self
            .file
            .as_ref()
            .map(|f| {
                ohl_structure_edit::groups(f.structure())
                    .into_iter()
                    .map(|g| g.name)
                    .collect()
            })
            .unwrap_or_default();
        let picked = self
            .selected
            .iter()
            .filter(|h| matches!(h, Handle::Joint(_)))
            .count();
        let all = self.file.as_ref().map_or(0, |f| f.structure().nodes.len());
        egui::Window::new("Lay out joints")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                let dark = ui.visuals().dark_mode;
                ohl_ui::tool_window::intro(
                    ui,
                    "Places the member ends of the joints so that they fit: pulled back, moved \
                     sideways only where needed, onto the leg or onto plates.",
                    "Each member end bolting by one leg onto another member's flange is laid flat \
                     (turned only where a joint of it is then laid out and no flat end is \
                     spoilt) and its gauge line put through the work point (concentric); its end is \
                     pulled back until its steel clears the flange's heel and the members placed \
                     before it by the gap. Only where that would need more than the longest \
                     pull-back is it moved sideways, the smallest move that fits, up to the shift \
                     limit (the analysis takes the move as an eccentricity). Where every bolt \
                     lands on the flange it bolts on directly, else all the plane's members onto \
                     one plate. What does not fit is left as it is and listed for a hand \
                     detail, never forced. Bolts are not chosen here: an end without bolts gets \
                     its class's preset or the bolt picked (provisional). Nothing is written \
                     before Apply (one undo step); later edits near a laid-out joint lay it out \
                     again.",
                    LAYOUT_ANCHOR,
                    || crate::help::open_section(LAYOUT_PAGE.0, LAYOUT_PAGE.1),
                );
                egui::ScrollArea::vertical()
                    .id_salt("layout-window-body")
                    .max_height(560.0)
                    .show(ui, |ui| {
                ohl_ui::tool_window::heading(
                    ui,
                    "Joints",
                    "Which joints are laid out. Similar joints have the same member sections, \
                     classes and angles (within 1°); symmetric ones mirror them ([[symmetry]]).",
                );
                if let Scope::Joint(j) = &w.scope {
                    let j = j.clone();
                    ui.radio_value(&mut w.scope, Scope::Joint(j.clone()), format!("Joint {j}"));
                }
                ui.radio_value(
                    &mut w.scope,
                    Scope::Picked,
                    format!("The joints picked ({picked})"),
                );
                ui.horizontal(|ui| {
                    let in_group = matches!(w.scope, Scope::Group(_));
                    if ui.radio(in_group, "A member group's joints").clicked()
                        && !in_group
                        && let Some(g) = groups.first()
                    {
                        w.scope = Scope::Group(g.clone());
                    }
                    if let Scope::Group(g) = &mut w.scope {
                        egui::ComboBox::from_id_salt("layout-group")
                            .selected_text(g.clone())
                            .show_ui(ui, |ui| {
                                for x in &groups {
                                    ui.selectable_value(g, x.clone(), x);
                                }
                            });
                    }
                });
                ui.radio_value(
                    &mut w.scope,
                    Scope::Tower,
                    format!("The whole tower ({all} joints)"),
                );
                ui.add_enabled(
                    w.scope != Scope::Tower,
                    egui::Checkbox::new(&mut w.similar, "With their similar and symmetric joints"),
                );
                ui.label(
                    egui::RichText::new(format!("{} joint(s) laid out", targets.len()))
                        .small()
                        .weak(),
                );
                ohl_ui::tool_window::heading(
                    ui,
                    "Settings",
                    "Not from a standard: the owner gave no values (2026-11-18). The gap's \
                     starting value is invented (10 mm); the pull-back's and the shift limit's \
                     were measured on real towers (see the theory page). Each changes the \
                     layout below at once.",
                );
                egui::Grid::new("layout-settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ohl_ui::tool_window::label(
                            ui,
                            "Gap between steel (mm)",
                            "Between the steel of members that do not bolt to each other, and \
                             from their bolts' nuts. Not from a standard: 10 mm to start with.",
                        );
                        ui.add(
                            egui::DragValue::new(&mut w.gap_mm)
                                .range(0.0..=100.0)
                                .speed(0.5),
                        );
                        ui.end_row();
                        ohl_ui::tool_window::label(
                            ui,
                            "Longest pull-back (mm)",
                            "How far a member's steel may stop from the work point before it is \
                             moved sideways instead (the plate gets long). Not from a standard.",
                        );
                        ui.add(
                            egui::DragValue::new(&mut w.pull_mm)
                                .range(10.0..=2000.0)
                                .speed(1.0),
                        );
                        ui.end_row();
                        ohl_ui::tool_window::label(
                            ui,
                            "Shift limit (mm)",
                            "The largest sideways move of a gauge line off the work point (0: \
                             never moved). The analysis takes a move as an eccentricity (a rigid \
                             end offset of a frame member). Not from a standard.",
                        );
                        ui.add(
                            egui::DragValue::new(&mut w.shift_mm)
                                .range(0.0..=200.0)
                                .speed(0.5),
                        );
                        ui.end_row();
                        ohl_ui::tool_window::label(
                            ui,
                            "Bolt where an end has none",
                            "Two in one gauge line, provisional (marked so) until the code \
                             step chooses the bolts; a tower with provisional bolts stays at \
                             level 3. Their pitch and end distance, and the edge distance a \
                             bolt keeps in the flange where the design code gives none, are \
                             the structure's default minimums (Design › Analysis; default \
                             minimum, assumed, not a code's).",
                        );
                        ui.horizontal(|ui| {
                            let text = w.bolt.clone().unwrap_or_else(|| "typed".into());
                            egui::ComboBox::from_id_salt("layout-bolt")
                                .selected_text(text)
                                .show_ui(ui, |ui| {
                                    for b in &bolts.0.bolts {
                                        ui.selectable_value(&mut w.bolt, Some(b.id.clone()), &b.id);
                                    }
                                    ui.selectable_value(&mut w.bolt, None, "typed");
                                });
                            if w.bolt.is_none() {
                                ui.add(
                                    egui::DragValue::new(&mut w.diameter_mm)
                                        .range(6.0..=48.0)
                                        .suffix(" mm"),
                                );
                            }
                        });
                        ui.end_row();
                        for class in CLASSES {
                            ohl_ui::tool_window::label(
                                ui,
                                &format!("Preset for {class}"),
                                "A connection preset's bolts for an end of this class that has \
                                 none (before the bolt picked); provisional too.",
                            );
                            let now = w.presets.get(class).cloned();
                            let mut pick = now.clone();
                            egui::ComboBox::from_id_salt(format!("layout-preset-{class}"))
                                .selected_text(pick.clone().unwrap_or_else(|| "none".into()))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut pick, None, "none");
                                    for (n, _) in &presets {
                                        ui.selectable_value(&mut pick, Some(n.clone()), n);
                                    }
                                });
                            if pick != now {
                                match pick {
                                    Some(n) => {
                                        w.presets.insert(class.to_owned(), n);
                                    }
                                    None => {
                                        w.presets.remove(class);
                                    }
                                }
                            }
                            ui.end_row();
                        }
                        ohl_ui::tool_window::label(
                            ui,
                            "Plates at a corner",
                            "Members in both faces of a leg: one plate per face, each bolted to \
                             its flange of the leg (usual lattice practice), or the plates welded \
                             together.",
                        );
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut w.welded, false, "Each bolted to its flange");
                            ui.radio_value(&mut w.welded, true, "Welded together");
                        });
                        ui.end_row();
                    });
                ui.checkbox(&mut w.replace, "Replace values given by hand")
                    .on_hover_text(
                        "Off: a cut, shift, roll or what a member bolts onto given by hand is \
                         kept (the layout works round it). On: replaced by the layout's.",
                    );
                ui.separator();
                // The preview.
                let busy = w.job.is_some();
                if let Some(e) = &w.error {
                    crate::app::error_label(ui, e.clone());
                }
                match w.preview.as_ref().filter(|p| p.key == key) {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if busy {
                                "Working out the layout on a copy…"
                            } else {
                                "No joint to lay out."
                            });
                        });
                    }
                    Some(p) => {
                        layout_summary(ui, p);
                        in_line = layout_in_line(ui, p);
                        leg_places = layout_leg_splices(ui, p);
                        layout_changes(ui, p);
                        layout_close_ups(ui, &mut w.shown, &mut w.views, p, dark);
                        usage_asked =
                            layout_usage(ui, w.usage.as_ref(), p.key, w.usage_job.is_some());
                    }
                }
                });
                let ready = w
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.key == key && p.revision == revision);
                let n = w.preview.as_ref().map_or(0, |p| p.run.laid_out().count());
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                match buttons(
                    ui,
                    &format!("Apply ({n} joints)"),
                    ready && n > 0,
                    Some(if ready {
                        "Nothing to lay out"
                    } else {
                        "The preview is being worked out"
                    }),
                    Keys::Escape,
                ) {
                    Some(Pressed::Action) => apply = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if usage_asked {
            self.start_usage(&mut w, ctx);
        }
        // J9 (D15): pieces in line confirmed one part, one undo step; the
        // preview is worked out again on the file as edited.
        if let Some(list) = in_line
            && let Some(f) = self.file.as_mut()
        {
            let asked = list.len();
            let r = f.edit(&format!("Pieces in line one part ({asked})"), |doc, t| {
                ohl_structure_link::in_line::confirm(doc, t, &list)
            });
            self.message = Some(match r {
                Ok((n, not)) if not.is_empty() => (
                    format!("{n} pair(s) of pieces in line one part (one undo step)"),
                    false,
                ),
                Ok((n, not)) => (
                    format!(
                        "{n} pair(s) of pieces in line one part (one undo step); not: {}",
                        not.join("; ")
                    ),
                    true,
                ),
                Err(e) => (e, true),
            });
        }
        // J7 (the owner's decisions of 2026-11-19): the leg splice places
        // confirmed — the same section one part, a change of section
        // spliced — one undo step; the preview worked out again.
        if let Some((one, splices)) = leg_places {
            // (Provisional bolts where a piece has none: the preset for the
            // class `leg`, else the bolt picked.)
            let bolts = self.bolt_library();
            let presets = self.preset_list();
            let s = w.settings(&bolts.0);
            let (areas, _) = ohl_structure_link::presets_by_class(&w.presets, &presets, &bolts.0);
            let bolt = areas.get("leg").cloned().or(s.bolt);
            let asked = one.len() + splices.len();
            let r = self.file.as_mut().map(|f| {
                f.edit(&format!("Leg splice places ({asked})"), |doc, t| {
                    ohl_structure_link::leg_splices::confirm_places(
                        doc,
                        t,
                        &one,
                        &splices,
                        bolt.as_ref(),
                    )
                })
            });
            self.message = r.map(|r| match r {
                Ok((n, m, not)) => (
                    format!(
                        "{m} leg splice(s) written, {n} pair(s) of leg pieces one part (one undo \
                         step){}",
                        if not.is_empty() {
                            String::new()
                        } else {
                            format!("; not: {}", not.join("; "))
                        }
                    ),
                    !not.is_empty(),
                ),
                Err(e) => (e, true),
            });
        }
        if apply && let Some(p) = w.preview.take() {
            let laid = p.run.laid_out().count();
            let hand = p.run.for_hand().count();
            let plates: usize = p.run.laid_out().map(|o| o.plates.len()).sum();
            let text = p.run.text.clone();
            if let Some(f) = self.file.as_mut() {
                f.edit_alone(&format!("Lay out joints ({laid})"), |doc, _| {
                    if let Ok(d) = text.parse::<toml_edit::DocumentMut>() {
                        *doc = d;
                    }
                });
            }
            self.message = Some((
                format!(
                    "{laid} joint(s) laid out ({plates} plate(s), one undo step){}; clashes at \
                     them {} to {}",
                    if hand > 0 {
                        format!(", {hand} need a hand detail (Joint details says why)")
                    } else {
                        String::new()
                    },
                    p.run.clashes.0,
                    p.run.clashes.1
                ),
                false,
            ));
            return;
        }
        if open && !cancel {
            self.layout_window = Some(w);
        }
    }

    /// Starts the usage before and after for the last run's cases.
    fn start_usage(&mut self, w: &mut LayoutWindow, ctx: &egui::Context) {
        let Some(p) = w.preview.as_ref() else { return };
        let Some(f) = self.file.as_ref() else { return };
        let (before, after, key) = (f.text(), p.run.text.clone(), p.key);
        if let Some(rx) = self.usage_thread(before, after, key) {
            w.usage_job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    /// The members' largest usage over the last run's cases with the file
    /// `before_text` and `after_text`, worked out on a thread of its own (the
    /// analysis twice); none (said) when there is no run yet.
    pub(crate) fn usage_thread(
        &mut self,
        before_text: String,
        after_text: String,
        key: u64,
    ) -> Option<Receiver<Result<UsageCompare, String>>> {
        let f = self.file.as_ref()?;
        let (table, recipes) = self.ran_with_of();
        let names: Vec<String> = match self.results.results.as_ref() {
            Some(Ok(r)) => r.cases.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        };
        if names.is_empty() {
            self.message = Some((
                "no run yet: Results › Run first (the usage before and after takes the last \
                 run's cases)"
                    .into(),
                true,
            ));
            return None;
        }
        let cases: Vec<_> = ohl_structure_link::cases_to_run(&table, &recipes, None)
            .into_iter()
            .filter(|c| names.iter().any(|n| n == &c.name))
            .collect();
        let dir = f
            .path()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        let sections = self.loads.sections.clone();
        let bolts = self.bolt_library();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let usage = |text: &str| -> Result<BTreeMap<String, Option<f64>>, String> {
                let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
                    std::path::Path::new("structure.ohls"),
                    text,
                )
                .map_err(|e| e.message)?;
                let run = ohl_structure_link::analyse_structure(
                    &ohl_structure_link::RunInput {
                        structure: &t,
                        text,
                        dir: dir.as_deref(),
                        sections: &sections,
                        table: &table,
                        recipes: &recipes,
                        cases: &cases,
                        extension: None,
                        bolts: Some(&bolts.0),
                        feet: None,
                    },
                    &ohl_core::budget::Budget::new(),
                )?;
                let mut out: BTreeMap<String, Option<f64>> = BTreeMap::new();
                for c in &run.cases {
                    if let Some(Ok(r)) = &c.fe {
                        for m in &r.members {
                            let e = out.entry(m.id.clone()).or_insert(None);
                            if let Some(u) = m.usage {
                                *e = Some(e.map_or(u, |x: f64| x.max(u)));
                            }
                        }
                    }
                }
                Ok(out)
            };
            let r = (|| {
                let a = usage(&before_text)?;
                let b = usage(&after_text)?;
                let mut rows: Vec<(String, Option<f64>, Option<f64>)> = a
                    .iter()
                    .map(|(m, u)| (m.clone(), *u, b.get(m).copied().flatten()))
                    .collect();
                // The largest change first.
                rows.sort_by(|x, y| {
                    let d = |r: &(String, Option<f64>, Option<f64>)| {
                        (r.2.unwrap_or(0.0) - r.1.unwrap_or(0.0)).abs()
                    };
                    d(y).total_cmp(&d(x)).then(x.0.cmp(&y.0))
                });
                Ok(UsageCompare {
                    key,
                    rows,
                    cases: cases.len(),
                    seconds: start.elapsed().as_secs_f64(),
                })
            })();
            let _ = tx.send(r);
        });
        Some(rx)
    }

    fn poll_usage(&mut self, w: &mut LayoutWindow, ctx: &egui::Context) {
        if let Some(e) = poll_usage_job(&mut w.usage_job, &mut w.usage, ctx) {
            self.message = Some((format!("usage before and after: {e}"), true));
        }
    }

    /// The layout follows later edits: the file's follow set (again when
    /// the libraries or presets change) to [`ohl_structure_link::follow`].
    pub(crate) fn follow_setup(&mut self) {
        let stamp = self.libs_stamp;
        let has = self.file.as_ref().is_some_and(|f| f.has_follow());
        if has && self.follow_stamp == Some(stamp) {
            return;
        }
        if self.file.is_none() {
            return;
        }
        let sections = self.loads.sections.clone();
        let bolts = self.bolt_library();
        let presets = self.preset_list();
        // The file as the edit left it (not read again: quick on a large
        // tower).
        let hook: ohl_structure_edit::AfterEdit = std::sync::Arc::new(move |e| {
            ohl_structure_link::follow_read(
                e.before,
                e.after_text,
                e.after.clone(),
                &sections,
                &bolts.0,
                &presets,
            )
            .map(|f| (f.text, f.message))
        });
        if let Some(f) = self.file.as_mut() {
            f.set_follow(Some(hook));
        }
        self.follow_stamp = Some(stamp);
    }
}

/// The numbers of a preview.
fn layout_summary(ui: &mut egui::Ui, p: &Preview) {
    let laid = p.run.laid_out().count();
    let hand = p.run.for_hand().count();
    let plates: usize = p.run.laid_out().map(|o| o.plates.len()).sum();
    let provisional: usize = p
        .run
        .laid_out()
        .flat_map(|o| &o.plan.ends)
        .filter(|e| e.bolts.is_some())
        .count();
    // Member ends laid out of those the layout places (J2: the ends that
    // fit are kept in a joint that is not whole).
    let ends: usize = p.run.laid_out().map(|o| o.plan.ends.len()).sum();
    let all: usize = p
        .run
        .joints
        .iter()
        .map(|o| o.plan.ends.len() + o.plan.hand_members.len())
        .sum();
    // J4: the joints laid out as a joint like them (their plan mapped).
    let like = p.run.laid_out().filter(|o| o.plan.like.is_some()).count();
    ui.label(format!(
        "{laid} joint(s) laid out, {plates} plate(s); {hand} need a hand detail; {ends} of {all} \
         member end(s) laid out; clashes at these joints {} to {}; {provisional} end(s) with \
         provisional bolts. Worked out in {:.2} s (measured).",
        p.run.clashes.0, p.run.clashes.1, p.run.seconds
    ))
    .on_hover_text(format!(
        "{like} of the joints laid out took the plan of a joint like them (the tower's symmetry, \
         a repeated panel: planned once per kind, checked for clashes in their own place)"
    ));
    let shifted = p
        .run
        .laid_out()
        .flat_map(|o| &o.plan.ends)
        .filter(|e| e.shift.is_some_and(|s| s.abs() >= 5e-5))
        .count();
    if shifted > 0 {
        ui.label(
            egui::RichText::new(format!(
                "{shifted} end(s) moved sideways: the analysis takes each move as an \
                 eccentricity where a frame member meets the joint; where only bars meet it, it \
                 is left out — NOT ON THE SAFE SIDE there (said per member in the results)"
            ))
            .small()
            .color(ui.visuals().warn_fg_color),
        );
    }
    for n in p.run.notes.iter().take(3) {
        ui.label(egui::RichText::new(n).small().weak());
    }
    if p.run.notes.len() > 3 {
        ui.label(
            egui::RichText::new(format!("… {} more notes", p.run.notes.len() - 3))
                .small()
                .weak(),
        );
    }
}

/// The help of the pieces in line (O44 J9).
pub const IN_LINE_HELP: &str = "Two pieces of one section in line through a joint (within 1°, \
     turned alike within 2°), with nothing given there of how they are joined: the layout proposes \
     them one part (continuous) for you to confirm. Until confirmed each piece ends at the joint \
     in the joint checks and in the layout, its steel from its cut (on the safe side) — never a \
     silent default. Confirming writes both ends one part in one undo step (rolls a little \
     apart: both the roll half-way between); the preview is then worked out again. Pieces of a \
     leg with bolts are a leg splice (a command of its own, to come) and are not proposed.";

/// The pieces in line of a preview (O44 J9, D15): those proposed one part,
/// each joint's with a button, and *Confirm all*; those not proposed with
/// why. The pieces to confirm when a button was pressed.
fn layout_in_line(ui: &mut egui::Ui, p: &Preview) -> Option<Vec<ohl_structure_link::InLine>> {
    // (The pieces of a leg with bolts are leg splice places: J7's list.)
    let list: Vec<&ohl_structure_link::InLine> =
        p.run.in_line.iter().filter(|x| !x.splice_bolts).collect();
    if list.is_empty() {
        return None;
    }
    let mut out = None;
    let yes: Vec<&ohl_structure_link::InLine> =
        list.iter().copied().filter(|x| x.proposed()).collect();
    let no: Vec<&ohl_structure_link::InLine> =
        list.iter().copied().filter(|x| !x.proposed()).collect();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.strong(format!(
            "Pieces in line: {} pair(s) proposed one part",
            yes.len()
        ))
        .on_hover_text(IN_LINE_HELP);
        ohl_ui::tool_window::question(ui, IN_LINE_HELP);
        if !yes.is_empty()
            && ui
                .button(format!("Confirm all ({})", yes.len()))
                .on_hover_text("Writes every proposed pair one part (continuous), one undo step")
                .clicked()
        {
            out = Some(yes.iter().map(|x| (*x).clone()).collect());
        }
    });
    egui::CollapsingHeader::new(format!(
        "Each joint ({} proposed, {} not)",
        yes.len(),
        no.len()
    ))
    .id_salt("layout-in-line")
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("layout-in-line-list")
            .max_height(160.0)
            .show(ui, |ui| {
                let mut joints: Vec<&str> = Vec::new();
                for x in &yes {
                    if !joints.contains(&x.joint.as_str()) {
                        joints.push(&x.joint);
                    }
                }
                for j in joints {
                    let here: Vec<&ohl_structure_link::InLine> =
                        yes.iter().copied().filter(|x| x.joint == j).collect();
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("Confirm")
                            .on_hover_text("These pieces one part (one undo step)")
                            .clicked()
                        {
                            out = Some(here.iter().map(|x| (*x).clone()).collect());
                        }
                        let words: Vec<String> = here
                            .iter()
                            .map(|x| match x.also() {
                                Some(a) => format!("{} and {} ({a})", x.member, x.onto),
                                None => format!("{} and {}", x.member, x.onto),
                            })
                            .collect();
                        ui.label(format!("{j}: {} one part?", words.join(", ")));
                    });
                }
                for x in &no {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}: not proposed — {}",
                            x.words(),
                            x.not.as_deref().unwrap_or_default()
                        ))
                        .small()
                        .weak(),
                    );
                }
            });
    });
    out
}

/// The leg splice places to confirm: the pieces of one section one part,
/// the splices.
type LegPlaces = (
    Vec<ohl_structure_link::InLine>,
    Vec<ohl_structure_link::LegSplice>,
);

/// The help of the leg splice places (O44 J7).
pub const LEG_SPLICES_HELP: &str = "Where a leg's pieces meet in line (the owner's rule of \
     2026-11-19): a change of section is a splice — proposed with covers outer and inner on each \
     leg bolted, as thick as the thicker piece's leg (the program's starting value, checked as \
     plates), the import's bolts, else provisional ones; the same section is one part unless \
     marked — proposed one part (its pieces' bolts then not checked there: not on the safe side \
     if a splice of one section is really there; mark it by giving the splice by hand). Never \
     written silently: until confirmed each piece ends at the joint in the joint checks and in \
     the layout (on the safe side). Confirmed, bracing at the joint may bolt through the \
     splice's outer cover. One undo step; the preview is then worked out again.";

/// The leg splice places of a preview (O44 J7): the splices and the pieces
/// of one section proposed, each joint's with a button, and *Confirm all*;
/// those not proposed with why. What to confirm when a button was pressed.
fn layout_leg_splices(ui: &mut egui::Ui, p: &Preview) -> Option<LegPlaces> {
    let one: Vec<&ohl_structure_link::InLine> = p
        .run
        .in_line
        .iter()
        .filter(|x| x.splice_bolts && x.proposed())
        .collect();
    let yes: Vec<&ohl_structure_link::LegSplice> =
        p.run.splices.iter().filter(|s| s.proposed()).collect();
    let no: Vec<&ohl_structure_link::LegSplice> =
        p.run.splices.iter().filter(|s| !s.proposed()).collect();
    if one.is_empty() && p.run.splices.is_empty() {
        return None;
    }
    let mut out = None;
    let all = one.len() + yes.len();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.strong(format!(
            "Leg splices: {} proposed spliced, {} one part",
            yes.len(),
            one.len()
        ))
        .on_hover_text(LEG_SPLICES_HELP);
        ohl_ui::tool_window::question(ui, LEG_SPLICES_HELP);
        if all > 0
            && ui
                .button(format!("Confirm all ({all})"))
                .on_hover_text(
                    "Writes every proposed leg splice and every leg's pieces of one section one \
                     part, one undo step",
                )
                .clicked()
        {
            out = Some((
                one.iter().map(|x| (*x).clone()).collect(),
                yes.iter().map(|x| (*x).clone()).collect(),
            ));
        }
    });
    egui::CollapsingHeader::new(format!("Each leg joint ({all} proposed, {} not)", no.len()))
        .id_salt("layout-leg-splices")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("layout-leg-splices-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    let mut joints: Vec<&str> = Vec::new();
                    for j in yes
                        .iter()
                        .map(|s| s.joint.as_str())
                        .chain(one.iter().map(|x| x.joint.as_str()))
                    {
                        if !joints.contains(&j) {
                            joints.push(j);
                        }
                    }
                    for j in joints {
                        let s_here: Vec<&ohl_structure_link::LegSplice> =
                            yes.iter().copied().filter(|s| s.joint == j).collect();
                        let o_here: Vec<&ohl_structure_link::InLine> =
                            one.iter().copied().filter(|x| x.joint == j).collect();
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("Confirm")
                                .on_hover_text("This joint's leg splice place (one undo step)")
                                .clicked()
                            {
                                out = Some((
                                    o_here.iter().map(|x| (*x).clone()).collect(),
                                    s_here.iter().map(|x| (*x).clone()).collect(),
                                ));
                            }
                            let mut words: Vec<String> = s_here
                                .iter()
                                .map(|s| format!("{} and {} spliced?", s.member, s.onto))
                                .collect();
                            words.extend(o_here.iter().map(|x| match x.also() {
                                Some(a) => format!(
                                    "{} and {} one part? (the same section; {a})",
                                    x.member, x.onto
                                ),
                                None => format!(
                                    "{} and {} one part? (the same section)",
                                    x.member, x.onto
                                ),
                            }));
                            ui.label(format!("{j}: {}", words.join("; ")));
                        });
                        // What is assumed, wrapped below the row.
                        for s in &s_here {
                            ui.label(
                                egui::RichText::new(format!("Assumed: {}", s.assumed()))
                                    .small()
                                    .weak(),
                            );
                        }
                    }
                    for s in &no {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: not proposed — {}",
                                s.words(),
                                s.not.as_deref().unwrap_or_default()
                            ))
                            .small()
                            .weak(),
                        );
                    }
                });
        });
    out
}

/// What changes per joint, and the joints left for a hand detail.
fn layout_changes(ui: &mut egui::Ui, p: &Preview) {
    let laid: Vec<&ohl_structure_link::JointOutcome> = p.run.laid_out().collect();
    egui::CollapsingHeader::new(format!("What changes ({} joints)", laid.len()))
        .id_salt("layout-changes")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("layout-changes-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for o in &laid {
                        ui.label(change_words(o));
                    }
                });
        });
    let hand: Vec<&ohl_structure_link::JointOutcome> = p.run.for_hand().collect();
    egui::CollapsingHeader::new(format!("Needs a hand detail ({} joints)", hand.len()))
        .id_salt("layout-hand")
        .default_open(!hand.is_empty() && hand.len() <= 5)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("layout-hand-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for o in &hand {
                        ui.label(
                            egui::RichText::new(hand_words(o)).color(ui.visuals().warn_fg_color),
                        );
                    }
                });
        });
}

/// A joint left for a hand detail in words (J2, 2026-11-19): which members
/// are left — the rest of the joint laid out — and whose place is kept free
/// (every end laid out keeps clear of it), then why: "N2: by hand R1 (its
/// place kept free), the rest laid out — R1: too short …"; a joint left
/// whole: "N3: left whole — …".
pub fn hand_words(o: &ohl_structure_link::JointOutcome) -> String {
    let why = o.hand.join("; ");
    if !o.written {
        return format!("{}: left whole — {why}", o.joint);
    }
    let members: Vec<String> = o
        .plan
        .hand_members
        .iter()
        .map(|m| {
            if o.plan.kept_free.contains(m) {
                format!("{m} (its place kept free)")
            } else {
                m.clone()
            }
        })
        .collect();
    if members.is_empty() {
        return format!("{}: {why}", o.joint);
    }
    format!(
        "{}: by hand {}, the rest laid out — {why}",
        o.joint,
        members.join(", ")
    )
}

/// One joint's changes in words: "N2: D1 steel stops at 34 mm, onto L2;
/// D2 … ; plate G1 7 mm (D1, D2)".
pub fn change_words(o: &ohl_structure_link::JointOutcome) -> String {
    let mut parts: Vec<String> = o
        .plan
        .ends
        .iter()
        .map(|e| {
            let mut w = e.member.clone();
            if let Some(c) = e.cut {
                w.push_str(&format!(" steel stops at {:.0} mm", c * 1000.0));
            }
            if let Some(s) = e.shift.filter(|s| s.abs() >= 5e-5) {
                w.push_str(&format!(", shift {:+.1} mm", s * 1000.0));
            }
            if e.off.abs() > 1e-6 {
                w.push_str(&format!(
                    " (gauge line {:.0} mm off the work point)",
                    e.off.abs() * 1000.0
                ));
            }
            match &e.onto {
                ohl_structure_link::Onto::Member(m) => w.push_str(&format!(", onto {m}")),
                ohl_structure_link::Onto::Plate(k) => {
                    let id = o.plates.get(*k).cloned().unwrap_or_default();
                    w.push_str(&format!(", onto plate {id}"));
                }
            }
            if let Some(b) = &e.bolts {
                w.push_str(&format!(", {} provisional bolt(s)", b.count));
            }
            w
        })
        .collect();
    for (id, pl) in o.plates.iter().zip(&o.plan.plates) {
        parts.push(format!(
            "plate {id} {:.0} mm on {} ({})",
            pl.thickness * 1000.0,
            pl.on,
            pl.members.join(", ")
        ));
    }
    if !o.turned.is_empty() {
        parts.push(format!("turned flat: {}", o.turned.join(", ")));
    }
    format!("{}: {}", o.joint, parts.join("; "))
}

/// Each joint's close-up before and after (turned together by a drag).
fn layout_close_ups(
    ui: &mut egui::Ui,
    shown: &mut Option<String>,
    views: &mut (
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    ),
    p: &Preview,
    dark: bool,
) {
    let laid: Vec<String> = p.run.laid_out().map(|o| o.joint.clone()).collect();
    if laid.is_empty() {
        return;
    }
    let mut pick = shown
        .clone()
        .filter(|j| laid.contains(j))
        .unwrap_or_else(|| laid[0].clone());
    ui.horizontal(|ui| {
        ui.label("Before and after:");
        egui::ComboBox::from_id_salt("layout-shown")
            .selected_text(pick.clone())
            .height(240.0)
            .show_ui(ui, |ui| {
                for j in &laid {
                    ui.selectable_value(&mut pick, j.clone(), j);
                }
            });
    });
    *shown = Some(pick.clone());
    let (Some(a), Some(b)) = (p.before.joint(&pick), p.after.joint(&pick)) else {
        return;
    };
    before_after(ui, views, a, b, p.key, dark);
}

/// A joint's close-up before (`a`) and after (`b`) side by side, turned
/// together by a drag, each with its clashes counted (Lay out joints,
/// Solve the clashes).
pub(crate) fn before_after(
    ui: &mut egui::Ui,
    views: &mut (
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    ),
    a: &ohl_structure_link::JointDetail,
    b: &ohl_structure_link::JointDetail,
    key: u64,
    dark: bool,
) {
    let width = ((ui.available_width() - 8.0) / 2.0).max(120.0);
    let size = egui::vec2(width, 200.0);
    ui.horizontal(|ui| {
        for (k, j) in [(0u64, a), (1u64, b)] {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(if k == 0 { "Before" } else { "After" })
                        .small()
                        .strong(),
                );
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
                if response.dragged() {
                    let d = response.drag_delta();
                    for v in [&mut views.0, &mut views.1] {
                        v.yaw += d.x * 0.01;
                        v.pitch = (v.pitch + d.y * 0.01).clamp(-1.5, 1.5);
                    }
                }
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
                let view = if k == 0 { &mut views.0 } else { &mut views.1 };
                crate::close_up::paint(&painter, rect, j, view, (key, k), dark, 0);
                let clashes = j.clashes.len();
                ui.label(
                    egui::RichText::new(format!("{clashes} clash(es)"))
                        .small()
                        .color(if clashes > 0 {
                            ui.visuals().warn_fg_color
                        } else {
                            ui.visuals().weak_text_color()
                        }),
                );
            });
        }
    });
}

/// A usage job polled: its result into `usage` when done; the error when
/// it failed.
pub(crate) fn poll_usage_job(
    job: &mut Job<UsageCompare>,
    usage: &mut Option<UsageCompare>,
    ctx: &egui::Context,
) -> Option<String> {
    let (_, rx) = job.as_ref()?;
    match rx.try_recv() {
        Ok(Ok(u)) => {
            *usage = Some(u);
            *job = None;
            None
        }
        Ok(Err(e)) => {
            *job = None;
            Some(e)
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            None
        }
        Err(_) => {
            *job = None;
            None
        }
    }
}

/// The usage before and after (asked by a button: the analysis of the last
/// run's cases twice); true when asked.
pub(crate) fn layout_usage(
    ui: &mut egui::Ui,
    u: Option<&UsageCompare>,
    key: u64,
    busy: bool,
) -> bool {
    let mut asked = false;
    ui.horizontal(|ui| {
        ohl_ui::tool_window::label(
            ui,
            "Usage before and after",
            "Each member's largest usage over the last run's cases (Results › Run), as the file \
             is and as laid out: the shifts are rigid end offsets of frame members (moments), \
             the cuts and plates do not enter the members' check. The analysis runs twice.",
        );
        if busy {
            ui.spinner();
        } else if ui.button("Work it out").clicked() {
            asked = true;
        }
    });
    if let Some(u) = u.filter(|u| u.key == key) {
        let changed = u
            .rows
            .iter()
            .filter(|r| (r.2.unwrap_or(0.0) - r.1.unwrap_or(0.0)).abs() > 1e-6)
            .count();
        ui.label(format!(
            "{} case(s), {:.1} s (measured); {changed} member(s) change.",
            u.cases, u.seconds
        ));
        egui::ScrollArea::vertical()
            .id_salt("layout-usage")
            .max_height(120.0)
            .show(ui, |ui| {
                egui::Grid::new("layout-usage-grid")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Member");
                        ui.strong("Before");
                        ui.strong("After");
                        ui.end_row();
                        let pct = |x: Option<f64>| {
                            x.map_or("—".to_owned(), |v| format!("{:.1} %", v * 100.0))
                        };
                        for r in u.rows.iter().take(40) {
                            ui.label(&r.0);
                            ui.label(pct(r.1));
                            ui.label(pct(r.2));
                            ui.end_row();
                        }
                    });
            });
    }
    asked
}

impl StructureApp {
    /// Joint details' layout box: **Lay out joint**, and what the
    /// automatic layout set at joint `joint` (the values *automatic*, a run
    /// again or a later edit may replace them), the bolts provisional, a
    /// hand detail needed with why.
    pub(crate) fn layout_box(&mut self, ui: &mut egui::Ui, joint: &str) {
        let Some(f) = self.file.as_ref() else { return };
        let rec = LayoutRecord::of(f.structure());
        let mut open = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Lay out joint…")
                .on_hover_text(
                    "Places this joint's member ends so that they fit (pulled back, moved \
                     sideways only where needed, onto the leg or onto plates), with its similar \
                     and symmetric joints: a preview first, Apply one undo step",
                )
                .clicked()
            {
                open = true;
            }
            if rec.joints.contains(joint) {
                let auto: Vec<String> = rec
                    .ends
                    .iter()
                    .filter(|((_, j), f)| j == joint && !f.is_empty())
                    .map(|((m, _), f)| {
                        // (An `onto` given by hand the layout kept, L2's
                        // follow-up: said as yours.)
                        let words: Vec<String> = f
                            .iter()
                            .map(|x| match x.strip_prefix(ohl_structure_link::ONTO_HAND) {
                                Some(h) => format!("onto (yours: {h}, kept)"),
                                None => x.clone(),
                            })
                            .collect();
                        format!("{m}: {}", words.join(", "))
                    })
                    .chain(
                        rec.made
                            .iter()
                            .filter(|(_, (j, _))| j == joint)
                            .map(|(id, _)| format!("plate {id}")),
                    )
                    .collect();
                ui.label(egui::RichText::new("laid out automatically").italics())
                    .on_hover_text(format!(
                        "Automatic (the layout's; a run again or a later edit near this joint \
                         may replace them, a value you change becomes yours): {}",
                        auto.join("; ")
                    ));
            }
            let provisional = rec
                .provisional
                .iter()
                .filter(|(_, j)| j == joint)
                .map(|(m, _)| m.as_str())
                .collect::<Vec<_>>();
            if !provisional.is_empty() {
                ui.label(
                    egui::RichText::new(format!("provisional bolts: {}", provisional.join(", ")))
                        .italics()
                        .color(ui.visuals().warn_fg_color),
                )
                .on_hover_text(
                    "The layout's bolts (a class preset or the bolt picked), not chosen yet: the \
                     tower stays at level 3 for the joint details until none is provisional",
                );
            }
        });
        if let Some(why) = rec.hand.get(joint) {
            ui.label(
                egui::RichText::new(format!("Needs a hand detail: {why}"))
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
        }
        if open {
            self.open_layout_window(Scope::Joint(joint.to_owned()));
        }
    }
}
