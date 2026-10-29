// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The 3D layers of a model, as renderer scenes.

use ohl_render::Scene;

use crate::model::Model;
use crate::selection::Selection;

/// The layers of the 3D view, in drawing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Ground surface (the active terrain surface).
    Ground,
    /// Other terrain surfaces shown, each in its colour.
    Surfaces,
    /// LiDAR points (other than ground).
    Points,
    /// Route alignments.
    Alignment,
    /// Structures: stick drawings, insulator strings.
    Structures,
    /// Wires of the selected weather case.
    Wires,
    /// Wires of the other weather cases (faded).
    OtherCases,
    /// Saved project lines shown for comparison (Versions menu), each in its
    /// colour.
    OtherLines,
    /// Clearance violations: wire to target, red
    /// ([`View3d::marks`](crate::view3d::View3d::marks)).
    Clearances,
    /// A wire being strung (String wire tool).
    Draft,
    /// The selection, highlighted.
    Selection,
    /// Orthophotos (Line › Terrain › Backgrounds), under the plan view.
    Orthophotos,
    /// DXF drawings (Line › Terrain › Backgrounds), in the plan and 3D views.
    Drawings,
    /// Vegetation growing into the wires (Checks › Vegetation): a cyan dot
    /// joined to the wire.
    GrowIn,
    /// Danger trees (Checks › Vegetation): an orange trunk, a dashed line
    /// to the wire point it falls to.
    FallIn,
    /// Isoclearance lines on the ground (Checks › Vegetation).
    Isoclearance,
    /// Lines on the ground a distance beside the line (View › Lines on
    /// the ground beside the line, [`crate::ground_lines`]).
    GroundLines,
    /// The magnetic field (Checks › Electrical › Magnetic field): isolines
    /// from above, the cross-section panels' cuts, isosurfaces
    /// (translucent), each level in its colour ([`crate::field_overlay`]).
    MagneticField,
    /// Safety volumes around the wires (Checks › Safety volumes),
    /// translucent: drawn last.
    SafetyVolume,
    /// The rigid links of the attachment points at an offset from their
    /// joints (2026-10-21, [`crate::link_tool`]): magenta rods.
    RigidLinks,
}

impl Layer {
    /// All layers.
    pub const ALL: [Layer; 20] = [
        Layer::Ground,
        Layer::Surfaces,
        Layer::Points,
        Layer::Alignment,
        Layer::Structures,
        Layer::RigidLinks,
        Layer::Wires,
        Layer::OtherCases,
        Layer::OtherLines,
        Layer::Clearances,
        Layer::Draft,
        Layer::Selection,
        Layer::Orthophotos,
        Layer::Drawings,
        Layer::GrowIn,
        Layer::FallIn,
        Layer::Isoclearance,
        Layer::GroundLines,
        Layer::MagneticField,
        Layer::SafetyVolume,
    ];

    /// Name in the layer panel.
    pub fn name(self) -> &'static str {
        match self {
            Layer::Ground => "Ground",
            Layer::Surfaces => "Other surfaces",
            Layer::Points => "LiDAR points",
            Layer::Alignment => "Alignments",
            Layer::Structures => "Structures",
            Layer::Wires => "Wires (selected case)",
            Layer::OtherCases => "Wires (other cases)",
            Layer::OtherLines => "Other project lines",
            Layer::Clearances => "Clearance violations",
            Layer::Draft => "Wire being strung",
            Layer::Selection => "Selection",
            Layer::Orthophotos => "Orthophotos (plan)",
            Layer::Drawings => "Drawings (DXF)",
            Layer::GrowIn => "Vegetation: grow-in",
            Layer::FallIn => "Vegetation: danger trees",
            Layer::Isoclearance => "Isoclearance lines",
            Layer::GroundLines => "Ground lines (beside the line)",
            Layer::MagneticField => "Magnetic field (isolines, surfaces)",
            Layer::SafetyVolume => "Safety volumes",
            Layer::RigidLinks => "Rigid links (attachment points)",
        }
    }
}

/// The groups of the Layers list (owner: OHL-Design UI review,
/// 2026-10-26): each foldable, the folded ones kept between runs.
pub const LAYER_GROUPS: [(&str, &[Layer]); 4] = [
    (
        "Ground & survey",
        &[
            Layer::Ground,
            Layer::Surfaces,
            Layer::Points,
            Layer::GroundLines,
        ],
    ),
    (
        "Line",
        &[
            Layer::Alignment,
            Layer::Structures,
            Layer::RigidLinks,
            Layer::Wires,
            Layer::OtherCases,
            Layer::OtherLines,
            Layer::Draft,
            Layer::Selection,
        ],
    ),
    (
        "Results",
        &[
            Layer::Clearances,
            Layer::GrowIn,
            Layer::FallIn,
            Layer::Isoclearance,
            Layer::MagneticField,
            Layer::SafetyVolume,
        ],
    ),
    ("Backgrounds", &[Layer::Orthophotos, Layer::Drawings]),
];

/// The groups of the Layers list folded (by name), kept between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerGroups {
    /// The names of the groups folded.
    pub folded: std::collections::BTreeSet<String>,
}

impl LayerGroups {
    const KEY: &str = "layer-groups-folded";

    /// As kept (none: every group open).
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        let folded = storage
            .get_string(Self::KEY)
            .map(|s| {
                s.split(',')
                    .filter(|g| !g.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self { folded }
    }

    /// Kept between runs.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let list: Vec<&str> = self.folded.iter().map(String::as_str).collect();
        storage.set_string(Self::KEY, list.join(","));
    }

    /// The layers' ticks in their groups, each group a folding header
    /// with how many of its layers show; `visible` changed by a tick.
    pub fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        visible: &mut std::collections::HashSet<Layer>,
    ) {
        use eframe::egui;
        for (name, layers) in LAYER_GROUPS {
            let on = layers.iter().filter(|l| visible.contains(l)).count();
            let open = !self.folded.contains(name);
            let r = egui::CollapsingHeader::new(format!("{name} ({on}/{})", layers.len()))
                .id_salt(("layer-group", name))
                .open(Some(open))
                .show(ui, |ui| {
                    for layer in layers {
                        let mut on = visible.contains(layer);
                        if ui.checkbox(&mut on, layer.name()).changed() {
                            if on {
                                visible.insert(*layer);
                            } else {
                                visible.remove(layer);
                            }
                        }
                    }
                });
            if r.header_response.clicked() {
                if open {
                    self.folded.insert(name.to_owned());
                } else {
                    self.folded.remove(name);
                }
            }
        }
    }
}

/// What a line of a structure drawing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The mast drawn when there is no stick drawing.
    Mast,
    /// A member: of the stick drawing, or an arm to an insulator.
    Member,
    /// An insulator string (or a V-string leg).
    Insulator,
}

/// Where a structure's type is placed: its position raised by its height
/// adjustment (the line's one rule, `ohl_line::links::feet`).
pub fn type_origin(st: &ohl_line::Structure) -> [f64; 3] {
    ohl_line::links::feet(st)
}

/// The body a structure is drawn with: its type's at level 2 or 3 on its
/// extension ([`ohl_structures_fe::drawn`]); none at level 1 or without a
/// type.
pub fn drawn_body<'a>(
    bodies: &'a ohl_structures_fe::drawn::Bodies,
    st: &ohl_line::Structure,
) -> Option<&'a ohl_structures_fe::drawn::DrawnBody> {
    bodies.get(st.structure_type.as_deref()?, st.extension.as_deref())
}

/// The members of a structure as placed (levels 2 and 3), each with its
/// id (none: a body polyline); empty at level 1.
pub fn placed_members(
    bodies: &ohl_structures_fe::drawn::Bodies,
    st: &ohl_line::Structure,
) -> Vec<(Option<String>, Vec<[f64; 3]>)> {
    let Some(b) = drawn_body(bodies, st) else {
        return Vec::new();
    };
    b.members
        .iter()
        .map(|m| m.id.clone())
        .zip(b.placed_lines(type_origin(st), st.orientation_deg))
        .collect()
}

/// Gives the structures of drawing `d` (of `line`) their members and
/// poles' outlines at levels 2 and 3 (`bodies`): drawn on their own layer
/// instead of the symbol.
pub fn set_drawn_members(
    d: &mut ohl_drawing::RouteDrawing,
    line: &ohl_line::Line,
    bodies: &ohl_structures_fe::drawn::Bodies,
) {
    d.set_members(|id| {
        let st = line.structure(id)?;
        drawn_body(bodies, st)
            .map(|b| b.placed_lines(type_origin(st), st.orientation_deg))
            .filter(|l| !l.is_empty())
    });
    d.set_poles(|id| {
        let st = line.structure(id)?;
        drawn_body(bodies, st)
            .map(|b| b.placed_pole_rings(type_origin(st), st.orientation_deg))
            .filter(|r| !r.is_empty())
    });
}

/// The pole of a structure at level 2 or 3 as placed: its outer outline
/// at each height (closed), bottom to top; empty without one.
pub fn placed_pole(
    bodies: &ohl_structures_fe::drawn::Bodies,
    st: &ohl_line::Structure,
) -> Vec<Vec<[f64; 3]>> {
    drawn_body(bodies, st).map_or_else(Vec::new, |b| {
        b.placed_pole_rings(type_origin(st), st.orientation_deg)
    })
}

/// The lines a structure is drawn with (and picked by): its members
/// (levels 2 and 3, `bodies`), else its stick drawing, else a mast up to
/// the highest point and arms out to its points; then its strings as given
/// (from their tops to their wire points).
pub fn structure_lines(
    bodies: &ohl_structures_fe::drawn::Bodies,
    st: &ohl_line::Structure,
) -> Vec<(Part, Vec<[f64; 3]>)> {
    let mut out = Vec::new();
    let members = placed_members(bodies, st);
    if !members.is_empty() {
        for (_, m) in members {
            out.push((Part::Member, m));
        }
    } else if st.members.is_empty() {
        let top = st
            .points
            .iter()
            .map(|p| p.position[2])
            .fold(st.position[2], f64::max);
        let mast = |z: f64| [st.position[0], st.position[1], z];
        out.push((Part::Mast, vec![st.position, mast(top)]));
        for p in &st.points {
            out.push((Part::Member, vec![mast(p.position[2]), p.position]));
        }
    } else {
        for m in &st.members {
            out.push((Part::Member, m.clone()));
        }
    }
    for x in &st.strings {
        if let (Some(top), Some(wire)) = (st.node(&x.top), st.wire(&x.wire)) {
            out.push((Part::Insulator, vec![top.position(), wire.position]));
        }
    }
    out
}

/// A solved jumper's colour (its own, else its section's) and its
/// subconductor polylines, ending on the yokes at its ends
/// ([`ohl_line::bundles`]).
pub fn jumper_lines(
    line: &ohl_line::Line,
    j: &ohl_line::jumper::JumperShape,
    bundles: &ohl_line::bundles::BundleLines,
) -> ([u8; 3], Vec<Vec<[f64; 3]>>) {
    let def = line.jumpers.iter().find(|d| d.id == j.jumper);
    let col = def
        .and_then(|d| d.color.clone())
        .or_else(|| {
            def.and_then(|d| line.jumper_section(d))
                .map(|s| line.section_color(s))
        })
        .map_or([200, 200, 200], |c| rgb(&c));
    let lines = match (bundles.jumpers.get(&j.jumper), def) {
        (Some(l), _) => l.clone(),
        (None, Some(d)) => line.jumper_bundle(d).polylines(&j.points),
        (None, None) => vec![j.points.clone()],
    };
    (col, lines)
}

/// `#rrggbb` → sRGB bytes (grey if not a colour).
pub fn rgb(hex: &str) -> [u8; 3] {
    ohl_core::text::parse_rgb(hex).unwrap_or([176, 184, 192])
}

/// Display colour of a LiDAR class, by its roles (noise magenta: shown
/// unless hidden in Line › Terrain › LiDAR classes shown).
pub fn class_color(scheme: &ohl_lidar::ClassScheme, code: u8) -> Option<[u8; 3]> {
    // The scheme's own colour, else one from the roles.
    if let Some(c) = scheme
        .class(code)
        .and_then(ohl_lidar::classes::ClassDef::rgb)
    {
        return Some(c);
    }
    role_color(scheme, code)
}

/// The colour of a class from its roles.
pub fn role_color(scheme: &ohl_lidar::ClassScheme, code: u8) -> Option<[u8; 3]> {
    let roles: Vec<&str> = scheme
        .class(code)
        .map(|c| c.roles.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let has = |r: &str| roles.iter().any(|x| x.contains(r));
    Some(if has("noise") {
        [200, 60, 200]
    } else if has("vegetation") {
        [70, 150, 70]
    } else if has("building") || has("substation") || has("tank") {
        [205, 95, 80]
    } else if has("water") {
        [70, 130, 210]
    } else if has("conductor") || has("wire") || has("jumper") {
        [235, 205, 70]
    } else if has("railway") || has("ropeway") {
        [180, 120, 200]
    } else if has("road") || has("bridge") {
        [150, 150, 150]
    } else if has("structure") || has("pole") || has("insulator") {
        [230, 230, 230]
    } else if has("ground") {
        [150, 125, 95]
    } else {
        [120, 160, 170]
    })
}

/// Off the route worked on (View › Work on route): grey and faint.
const FADED: [u8; 4] = [130, 134, 140, 70];

fn rgba(c: [u8; 3], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// A polyline to draw, with the levels of detail it is drawn at: the
/// layers' common output, drawn whole ([`layer`]) or by chunk
/// ([`crate::chunks`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Prim {
    /// Where it belongs in plan (its chunk): a structure's position, a
    /// span's middle.
    pub anchor: [f64; 2],
    /// Points.
    pub points: Vec<[f64; 3]>,
    /// Colour.
    pub color: [u8; 4],
    /// Width, pixels.
    pub width: f32,
    /// Lowest and highest level of detail it is drawn at.
    pub levels: (usize, usize),
    /// Thinned to the layer's points per span at levels 1 and 2 (wires).
    pub thinned: bool,
    /// The points are triangles (three by three), drawn filled and lit,
    /// not a polyline: solid bodies (marker balls).
    pub filled: bool,
    /// A dot `width` px round at its first point, not a polyline (a body
    /// seen from afar).
    pub dot: bool,
    /// Drawn only within the solids distance
    /// ([`crate::lod::LodSettings::solids`]), whatever its level: the
    /// solids of members with a library profile and of poles.
    pub near: bool,
}

impl Prim {
    fn new(anchor: [f64; 3], points: Vec<[f64; 3]>, color: [u8; 4], width: f32) -> Self {
        Self {
            anchor: [anchor[0], anchor[1]],
            points,
            color,
            width,
            levels: (0, 0),
            thinned: false,
            filled: false,
            dot: false,
            near: false,
        }
    }

    fn at(mut self, lo: usize, hi: usize) -> Self {
        self.levels = (lo, hi);
        self
    }
}

/// Structures of `line` (solved in `cases`), their strings drawn to the
/// solved clamps in every weather case their section is shown in (strings
/// swing with the wind, move with temperature); strings without a section
/// hang as given. `tint`: one colour for everything (a project line).
///
/// Levels of detail: 0 everything; 1 mast and arms (a lattice: a mast line
/// up to its top attachment) and strings; 2 the mast line. The solids of
/// members with a library profile and of poles at levels 2 and 3 are
/// `near`: drawn within the solids distance only.
pub fn structures(
    out: &mut Vec<Prim>,
    model: &Model,
    line: &ohl_line::Line,
    cases: &[crate::model::Solved],
    case: usize,
    tint: Option<[u8; 3]>,
) {
    // The strings as solved (top, bottom, beyond their compression), by
    // structure and string.
    type Drawn = Vec<([f64; 3], [f64; 3], bool)>;
    let mut solved: std::collections::HashMap<(&str, String), Drawn> =
        std::collections::HashMap::new();
    // The volumes on the strings (a display option): placed on each
    // string as solved, by structure.
    let fits: Vec<ohl_clearance::internal::StringFit> = if model.section_display.string_volumes
        && tint.is_none()
    {
        let empty = ohl_structures::InsulatorLibrary::default();
        ohl_clearance::internal::string_fits(line, model.types.as_ref().map_or(&empty, |(_, i)| i))
    } else {
        Vec::new()
    };
    let fit_of: ohl_clearance::internal::FitIndex = fits
        .iter()
        .map(|f| ((f.structure.as_str(), f.label.as_str()), f))
        .collect();
    let mut volumes: std::collections::HashMap<&str, Vec<ohl_clearance::volumes::Placed>> =
        std::collections::HashMap::new();
    // Each case's shapes by section, made when first wanted (the first
    // shape of a section, as a search finds it).
    let mut by_case: std::collections::HashMap<
        *const crate::model::Solved,
        std::collections::HashMap<&str, &ohl_line::solve::SectionShape>,
    > = Default::default();
    for sec in &line.sections {
        // Both wind sides shown: the strings to each side's clamps.
        let drawn: Vec<&crate::model::Solved> = model
            .section_cases(&sec.id, case)
            .into_iter()
            .filter_map(|k| cases.get(k))
            .flat_map(|c| c.drawn())
            .collect();
        let mut shapes = Vec::new();
        for c in drawn {
            let index = by_case.entry(std::ptr::from_ref(c)).or_insert_with(|| {
                let mut m = std::collections::HashMap::with_capacity(c.shapes.len());
                for s in &c.shapes {
                    m.entry(s.section.as_str()).or_insert(s);
                }
                m
            });
            if let Some(s) = index.get(sec.id.as_str()) {
                shapes.push(*s);
            }
        }
        for shape in shapes {
            for (k, (p, strings)) in sec.points.iter().zip(&shape.strings).enumerate() {
                for x in strings {
                    solved
                        .entry((p.structure.as_str(), x.label.clone()))
                        .or_default()
                        .push((x.top, x.bottom, x.over));
                    if let Some(fit) = fit_of.get(&(p.structure.as_str(), x.label.as_str())) {
                        volumes
                            .entry(p.structure.as_str())
                            .or_default()
                            .extend(ohl_clearance::internal::placed_shapes(fit, x, shape, k));
                    }
                }
            }
        }
    }
    // Strings holding jumpers' support clamps: to the solved clamps, in
    // the cases the jumpers are drawn in.
    for c in model
        .shown(case)
        .into_iter()
        .filter_map(|k| cases.get(k))
        .flat_map(|c| c.drawn())
    {
        for s in c.jumpers.iter().flat_map(|j| &j.supports) {
            let Some(st) = line.structure(&s.structure) else {
                continue;
            };
            for x in &s.strings {
                solved
                    .entry((st.id.as_str(), x.label.clone()))
                    .or_default()
                    .push((x.top, s.clamp, false));
            }
        }
    }
    let scope = model.scope();
    for st in &line.structures {
        // Off the route worked on: faded.
        let faded = scope
            .as_ref()
            .is_some_and(|s| !s.structures.contains(&st.id));
        let pick = |c: [u8; 4]| if faded { FADED } else { c };
        // Coloured by usage (Checks › Structure loads › Usage): the steel.
        let by_usage = tint
            .is_none()
            .then(|| model.usage_marks.get(&st.id))
            .flatten()
            .map(|m| rgba(m.color(), 255));
        let steel =
            pick(by_usage.unwrap_or_else(|| tint.map_or([215, 215, 220, 255], |c| rgba(c, 230))));
        let blue = pick(tint.map_or([120, 180, 255, 255], |c| rgba(c, 230)));
        let at = st.position;
        let body = drawn_body(&model.bodies, st);
        let lattice = !st.members.is_empty() || body.is_some_and(|b| b.has_members());
        // Members with a profile (level 3) and poles at levels 2 and 3:
        // their solids within the solids distance (`near`), over the lines
        // they are drawn with from afar. The type's geometry is shared,
        // only moved here.
        if let Some(b) = body.filter(|b| b.triangles() > 0) {
            let mut s = Prim::new(
                at,
                b.placed_triangles(type_origin(st), st.orientation_deg),
                steel,
                0.0,
            )
            .at(0, 0);
            s.filled = true;
            s.near = true;
            out.push(s);
        }
        for (part, pts) in structure_lines(&model.bodies, st) {
            let prim = match part {
                // Stick drawings: the mast at every level, arms down to 1.
                Part::Mast => Prim::new(at, pts, steel, 2.5).at(0, 2),
                Part::Member if lattice => Prim::new(at, pts, steel, 1.5).at(0, 0),
                Part::Member => Prim::new(at, pts, steel, 1.5).at(0, 1),
                Part::Insulator => continue,
            };
            out.push(prim);
        }
        if lattice {
            // Levels 1 and 2 of a lattice: a mast line to its top.
            let top = st
                .points
                .iter()
                .map(|p| p.position[2])
                .chain(st.members.iter().flatten().map(|p| p[2]))
                .fold(at[2], f64::max);
            out.push(Prim::new(at, vec![at, [at[0], at[1], top]], steel, 2.5).at(1, 2));
        }
        // Unlinked attachment points (their joint gone from the type, or
        // unlinked): red, their structure's loads refused.
        if st.structure_type.is_some() && tint.is_none() {
            for p in st.points.iter().filter(|p| p.unlinked) {
                let mut dot =
                    Prim::new(at, vec![p.position], crate::link_tool::UNLINKED, 13.0).at(0, 1);
                dot.dot = true;
                out.push(dot);
            }
        }
        // The volumes on the strings: filled, within the solids distance,
        // by their electrical state.
        if let Some(placed) = volumes.get(st.id.as_str()) {
            use ohl_structures::ElectricalState;
            for (state, color) in [
                (ElectricalState::Grounded, [150, 150, 155, 255]),
                (ElectricalState::Electrified, [240, 150, 60, 255]),
                (ElectricalState::Insulated, [225, 220, 180, 255]),
            ] {
                let points: Vec<[f64; 3]> = placed
                    .iter()
                    .filter(|p| p.state == state)
                    .flat_map(|p| p.convex.mesh(12))
                    .flatten()
                    .collect();
                if points.is_empty() {
                    continue;
                }
                let mut s = Prim::new(at, points, pick(color), 0.0).at(0, 0);
                s.filled = true;
                s.near = true;
                out.push(s);
            }
        }
        // The strings, as solved in the cases shown (else as given);
        // flagged ones (beyond the compression they may take) in red.
        let red = pick(tint.map_or([235, 60, 50, 255], |c| rgba(c, 230)));
        for x in &st.strings {
            match solved.get(&(st.id.as_str(), x.label.clone())) {
                Some(drawn) => {
                    for (top, bottom, over) in drawn {
                        let col = if *over { red } else { blue };
                        out.push(Prim::new(at, vec![*top, *bottom], col, 2.0).at(0, 1));
                    }
                }
                None => {
                    if let (Some(top), Some(wire)) = (st.node(&x.top), st.wire(&x.wire)) {
                        out.push(
                            Prim::new(at, vec![top.position(), wire.position], blue, 2.0).at(0, 1),
                        );
                    }
                }
            }
        }
    }
}

/// The middle of a polyline's plan extent (its chunk).
fn middle(points: &[[f64; 3]]) -> [f64; 3] {
    let (a, b) = (points[0], points[points.len() - 1]);
    [
        0.5 * (a[0] + b[0]),
        0.5 * (a[1] + b[1]),
        0.5 * (a[2] + b[2]),
    ]
}

/// Wires of `line` solved in `cases`: each section in its own display
/// cases, bundles as subconductors ending on the yokes, jumpers in the
/// cases shown for the line. `shown`: the cases shown (else the others,
/// faint). `tint`: one colour for everything (a project line).
///
/// Levels of detail: 0 every subconductor and the yokes; 1 and 2 one line
/// per bundle (the solved wire), thinned to the layer's points per span.
pub fn wires(
    out: &mut Vec<Prim>,
    model: &Model,
    line: &ohl_line::Line,
    cases: &[crate::model::Solved],
    case: usize,
    shown: bool,
    tint: Option<[u8; 3]>,
) {
    let alpha = match (shown, tint) {
        (false, _) => 90,
        (true, Some(_)) => 230,
        (true, None) => 255,
    };
    // Colours and line codes (Line › Sections › Display options), the
    // directions as arrows in the case shown.
    let display = &model.section_display;
    let styles = crate::wire_style::Styles::new(line, display.coloring);
    let arrows = tint.is_none() && display.direction[crate::annotations::View::ThreeD as usize];
    let wire = |out: &mut Vec<Prim>,
                points: &[[f64; 3]],
                lines: Vec<Vec<[f64; 3]>>,
                color: [u8; 4],
                code: &[f64],
                w: [f32; 2]| {
        if points.len() < 2 {
            return;
        }
        let at = middle(points);
        let bundle = lines.len() > 1;
        for l in lines {
            for d in crate::wire_style::dashes(&l, code, DASH_UNIT) {
                out.push(Prim::new(at, d, color, if bundle { w[0] } else { w[1] }).at(0, 0));
            }
        }
        // Farther, one line per bundle, its line code's dashes longer
        // ([`crate::lod::DASH_SCALE`]: dashes of a metre and a half are
        // not seen kilometres away, thousands of them per span are drawn).
        if code.is_empty() {
            let mut centre = Prim::new(at, points.to_vec(), color, w[1]).at(1, 2);
            centre.thinned = true;
            out.push(centre);
        } else {
            for level in 1..=2 {
                let unit = DASH_UNIT * crate::lod::DASH_SCALE[level];
                for d in crate::wire_style::dashes(points, code, unit) {
                    let mut centre = Prim::new(at, d, color, w[1]).at(level, level);
                    centre.thinned = true;
                    out.push(centre);
                }
            }
        }
    };
    let arrow =
        |out: &mut Vec<Prim>, points: &[[f64; 3]], forward: bool, size: f64, color: [u8; 4]| {
            if let Some(legs) =
                crate::wire_style::chevron(points, forward, size, crate::wire_style::across)
            {
                for l in legs {
                    out.push(Prim::new(l[1], l.to_vec(), color, 2.0).at(0, 1));
                }
            }
        };
    // Off the route worked on: faded.
    let scope = model.scope();
    let fade = |faded: bool, c: [u8; 4]| if faded { FADED } else { c };
    // Both wind sides shown: each case's mirror drawn with it.
    for (k, side, solved) in cases
        .iter()
        .enumerate()
        .flat_map(|(k, c)| c.drawn().enumerate().map(move |(side, d)| (k, side, d)))
    {
        let this_case = shown == model.shown(case).contains(&k);
        // Sections not solved in this case (the working line): dashed red
        // where they were last solved (else another case's shape, else
        // their chords).
        if tint.is_none() {
            for (id, _) in &solved.unsolved {
                if shown != model.section_cases(id, case).contains(&k) {
                    continue;
                }
                let Some(drawn) = model.unsolved_drawn(id, k, side) else {
                    continue;
                };
                let col = rgba(UNSOLVED_COLOR, alpha);
                for points in drawn.spans.iter().filter(|p| p.len() > 1) {
                    let at = middle(points);
                    // Its dashes longer farther ([`crate::lod::DASH_SCALE`]).
                    for level in 0..=2 {
                        let unit = DASH_UNIT * crate::lod::DASH_SCALE[level];
                        for d in crate::wire_style::dashes(points, &UNSOLVED_DASHES, unit) {
                            let mut dash = Prim::new(at, d, col, 2.0).at(level, level);
                            dash.thinned = level > 0;
                            out.push(dash);
                        }
                    }
                }
            }
        }
        for shape in &solved.shapes {
            if shown != model.section_cases(&shape.section, case).contains(&k) {
                continue;
            }
            // Only the sections of `line` (a project line shown: what
            // differs from the line worked on).
            if tint.is_some() && !line.sections.iter().any(|s| s.id == shape.section) {
                continue;
            }
            let style = styles.section(&shape.section);
            let col = tint.unwrap_or(style.color);
            let faded = scope
                .as_ref()
                .is_some_and(|s| !s.sections.contains(&shape.section));
            let forward = styles.directions.sections.get(&shape.section).copied();
            for (n, span) in shape.spans.iter().enumerate() {
                // Bundles: each subconductor (one section, solved and
                // selected as one), ending on the yokes.
                let lines = match solved.bundles.spans.get(&(shape.section.clone(), n)) {
                    Some(l) => l.clone(),
                    None => vec![span.points.clone()],
                };
                wire(
                    out,
                    &span.points,
                    lines,
                    fade(faded, rgba(col, alpha)),
                    &style.pattern,
                    [1.5, 2.0],
                );
                if arrows
                    && this_case
                    && let Some(f) = forward
                {
                    arrow(out, &span.points, f, 6.0, fade(faded, rgba(col, alpha)));
                }
            }
            // Marker balls and other bodies with a size, on the wire
            // (the bundle's centre) where they hang.
            // (Places by station as fractions, by distance from the
            // span's plan length.)
            let ball = tint.unwrap_or(BALL_COLOR);
            let def = line
                .sections
                .iter()
                .find(|sec| sec.id == shape.section)
                .map(|sec| line.section_with_fractions(sec));
            let place = |span: usize, at: ohl_line::SpanPlace| {
                let points = &shape.spans.get(span)?.points;
                at_fraction(points, at.fraction_of(plan_length(points)))
            };
            // Spread objects: the stretch of wire they lie on.
            let spread = tint.unwrap_or(BALL_COLOR);
            for (_, stretch) in def.iter().flat_map(|sec| spreads_drawn(sec, shape)) {
                out.push(Prim::new(stretch[0], stretch, rgba(spread, alpha), 5.0).at(0, 1));
            }
            // Span points (as solved, else where they are given): their
            // bodies to size (a ball as big as the body), clamps in
            // steel, bare points as dots.
            let dot = tint.unwrap_or(SPAN_POINT_COLOR);
            let steel = tint.unwrap_or([205, 205, 210]);
            for p in def.iter().flat_map(|sec| &sec.span_points) {
                let at = shape
                    .span_points
                    .iter()
                    .find(|(l, _)| *l == p.label)
                    .map(|(_, q)| *q)
                    .or_else(|| place(p.span, p.at));
                let Some(at) = at else {
                    continue;
                };
                let points = shape.spans.get(p.span).map(|sp| sp.points.as_slice());
                let along = points.map_or([1.0, 0.0], chord_direction);
                // Its subconductors there (a clamp across a bundle joins
                // them).
                let subs = match (def.as_ref(), points) {
                    (Some(sec), Some(points)) => subconductors_at(sec, points, at),
                    _ => Vec::new(),
                };
                draw_point(
                    out,
                    at,
                    along,
                    p.object.as_ref(),
                    &subs,
                    [dot, steel, ball],
                    alpha,
                );
            }
            // The spacers it holds, red beyond their compression limit.
            for x in &shape.spacers {
                let col = match tint {
                    Some(c) => c,
                    None if x.over => [230, 40, 40],
                    None => SPACER_COLOR,
                };
                out.push(Prim::new(x.ends[0], x.ends.to_vec(), rgba(col, alpha), 4.0).at(0, 1));
            }
        }
        for j in solved
            .jumpers
            .iter()
            .filter(|_| this_case)
            .filter(|j| tint.is_none() || line.jumpers.iter().any(|x| x.id == j.jumper))
        {
            let (_, lines) = jumper_lines(line, j, &solved.bundles);
            let style = styles.jumper(&j.jumper);
            let col = tint.unwrap_or(style.color);
            let faded = scope
                .as_ref()
                .is_some_and(|s| !s.jumpers.contains(&j.jumper));
            wire(
                out,
                &j.points,
                lines,
                fade(faded, rgba(col, alpha)),
                &style.pattern,
                [1.2, 1.5],
            );
            if arrows && let Some(f) = styles.directions.jumpers.get(&j.jumper) {
                arrow(out, &j.points, *f, 1.5, fade(faded, rgba(col, alpha)));
            }
            // Its support clamps: a short bar along it.
            let steel = tint.unwrap_or([205, 205, 210]);
            let def = line.jumpers.iter().find(|d| d.id == j.jumper);
            // Its points and their objects.
            for (label, at) in &j.point_positions {
                let object = def
                    .and_then(|d| d.points.iter().find(|p| &p.label == label))
                    .and_then(|p| p.object.as_ref());
                draw_point(
                    out,
                    *at,
                    chord_direction(&j.points),
                    object,
                    &[],
                    [
                        tint.unwrap_or(SPAN_POINT_COLOR),
                        steel,
                        tint.unwrap_or(BALL_COLOR),
                    ],
                    alpha,
                );
            }
            for (s, c) in j
                .supports
                .iter()
                .zip(def.into_iter().flat_map(|d| d.supports()))
            {
                let h = c.clamp_length / 2.0;
                let (m, d) = (s.clamp, s.direction);
                let ends = vec![
                    [0, 1, 2].map(|k| m[k] - d[k] * h),
                    [0, 1, 2].map(|k| m[k] + d[k] * h),
                ];
                // Some decimetres long: close only (level 0).
                out.push(Prim::new(m, ends, rgba(steel, alpha), 4.0).at(0, 0));
            }
        }
        // Yokes at the clamps (a bar, a triangle, a polygon), with their
        // section, in its cases.
        let steel = tint.unwrap_or([205, 205, 210]);
        for y in solved
            .bundles
            .yokes
            .iter()
            .zip(&solved.bundles.yoke_sections)
            .filter(|(_, sec)| shown == model.section_cases(sec, case).contains(&k))
            .filter(|(_, sec)| tint.is_none() || line.sections.iter().any(|s| &s.id == *sec))
            .map(|(y, _)| y)
        {
            let mut ring = y.clone();
            if y.len() > 2 {
                ring.push(y[0]);
            }
            out.push(Prim::new(y[0], ring, rgba(steel, alpha), 2.0).at(0, 0));
        }
    }
}

/// Length of a unit of the phases' line codes in 3D, m.
const DASH_UNIT: f64 = 1.5;

/// Colour of a section not solved in a weather case, drawn in its place
/// ([`Model::unsolved_drawn`]): red, dashed ([`UNSOLVED_DASHES`]).
pub const UNSOLVED_COLOR: [u8; 3] = [235, 50, 50];

/// The dashes of a section not solved (units on, off).
pub const UNSOLVED_DASHES: [f64; 2] = [3.0, 2.0];

/// Colour of marker balls and other bodies on the wires (aviation
/// orange).
const BALL_COLOR: [u8; 3] = [240, 110, 30];

/// Every solved copy of section `section` drawn (its weather cases, both
/// wind sides), with its case.
fn drawn_copies<'a>(
    model: &'a Model,
    section: &str,
    case: usize,
) -> Vec<(usize, &'a crate::model::Solved)> {
    model
        .section_cases(section, case)
        .into_iter()
        .filter_map(|k| model.cases.get(k).map(|c| (k, c)))
        .flat_map(|(k, c)| c.drawn().map(move |d| (k, d)))
        .collect()
}

/// The spread objects of section `sec` (its places resolved,
/// [`ohl_line::Line::section_with_fractions`]) on its solved `shape`: each
/// one's index and the stretch of wire it lies on.
pub fn spreads_drawn(
    sec: &ohl_line::Section,
    shape: &ohl_line::solve::SectionShape,
) -> Vec<(usize, Vec<[f64; 3]>)> {
    sec.spread_loads()
        .into_iter()
        .filter_map(|l| {
            let points = &shape.spans.get(l.span)?.points;
            let h = plan_length(points);
            let stretch = between(points, l.start.fraction_of(h), l.end.fraction_of(h));
            (stretch.len() > 1).then_some((l.index, stretch))
        })
        .collect()
}

/// Where the subconductors of section `sec`'s bundle are at the point
/// `at` of a span's centre line `points` (none for a single wire or a
/// bundle without a spacing): around it in the plane across the wire
/// there ([`ohl_line::Section::bundle_offsets`]).
pub fn subconductors_at(
    sec: &ohl_line::Section,
    points: &[[f64; 3]],
    at: [f64; 3],
) -> Vec<[f64; 3]> {
    let offsets = sec.bundle_offsets();
    if offsets.len() < 2 {
        return Vec::new();
    }
    // The wire's direction there: the piece nearest to it.
    let near = points.windows(2).min_by(|a, b| {
        let d = |w: &[[f64; 3]]| {
            let m = [0, 1, 2].map(|k| 0.5 * (w[0][k] + w[1][k]) - at[k]);
            m[0] * m[0] + m[1] * m[1] + m[2] * m[2]
        };
        d(a).total_cmp(&d(b))
    });
    let Some(w) = near else {
        return Vec::new();
    };
    offsets
        .into_iter()
        .map(|o| ohl_line::bundle_point(at, w[0], w[1], o))
        .collect()
}

/// A rod of diameter `d` from `a` to `b` (12 sides, its ends closed) as
/// triangles facing out.
pub fn rod_mesh(a: [f64; 3], b: [f64; 3], d: f64) -> Vec<[f64; 3]> {
    use std::f64::consts::PI;
    let v = [0, 1, 2].map(|k| b[k] - a[k]);
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= 0.0 || d <= 0.0 {
        return Vec::new();
    }
    let u = v.map(|x| x / len);
    let cross = |x: [f64; 3], y: [f64; 3]| {
        [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ]
    };
    // p, q across it, p × q = u: the rings wound to face out.
    let p = {
        let c = cross(u, [0.0, 0.0, 1.0]);
        let n = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        if n > 1e-9 {
            c.map(|x| x / n)
        } else {
            [1.0, 0.0, 0.0]
        }
    };
    let q = cross(u, p);
    let r = d / 2.0;
    let n = 12;
    let ring = |c: [f64; 3], i: usize| {
        let t = 2.0 * PI * i as f64 / n as f64;
        [0, 1, 2].map(|k| c[k] + r * (t.cos() * p[k] + t.sin() * q[k]))
    };
    let mut t = Vec::new();
    for i in 0..n {
        let (a0, a1, b0, b1) = (ring(a, i), ring(a, i + 1), ring(b, i), ring(b, i + 1));
        t.extend([a0, a1, b1, a0, b1, b0]);
        t.extend([a, a1, a0]);
        t.extend([b, b0, b1]);
    }
    t
}

/// A clamp's solid: its own shape when given; a cylinder across the line
/// on a bundle (`subs`: its subconductors there) bars joining the
/// subconductors, as it holds them all (a ring of them for three or more);
/// else a short cylinder along the wire, its clamp length. Drawn only: its
/// forces take its shape as given ([`ohl_line::LineObject::forces`]).
fn clamp_mesh(
    o: &ohl_line::LineObject,
    at: [f64; 3],
    along: [f64; 2],
    subs: &[[f64; 3]],
) -> Vec<[f64; 3]> {
    if let ohl_line::Shape::Cylinder {
        diameter,
        axis: ohl_line::Axis::Across,
        ..
    } = o.shape
        && subs.len() >= 2
        && diameter > 0.0
    {
        let edges = if subs.len() == 2 { 1 } else { subs.len() };
        return (0..edges)
            .flat_map(|k| rod_mesh(subs[k], subs[(k + 1) % subs.len()], diameter))
            .collect();
    }
    let own = body_mesh(&o.shape, at, along);
    if !own.is_empty() {
        return own;
    }
    body_mesh(
        &ohl_line::Shape::Cylinder {
            diameter: 0.12,
            length: o.clamp_length.unwrap_or(0.2).max(0.1),
            axis: ohl_line::Axis::Along,
        },
        at,
        along,
    )
}

/// A point on a wire at `at`, by its object: a body to its shape and size
/// (none without a size), a clamp in steel ([`clamp_mesh`]; `subs`: the
/// subconductors there, on a bundle), a bare point a small cross marker —
/// a mark, not a body (`colors`: bare, clamp, body).
///
/// Levels of detail: clamps and marks close only (level 0, decimetres:
/// not seen farther); a body to its shape at level 0, a dot at level 1
/// ([`crate::lod::BODY_DOT_PX`]: marker balls stay seen from afar).
fn draw_point(
    out: &mut Vec<Prim>,
    at: [f64; 3],
    along: [f64; 2],
    object: Option<&ohl_line::LineObject>,
    subs: &[[f64; 3]],
    colors: [[u8; 3]; 3],
    alpha: u8,
) {
    let mut body = false;
    let (triangles, color) = match object {
        // A bare point: three short strokes (vertical, along, across the
        // wire), drawn as lines.
        None => {
            let l = 0.4;
            let n = (along[0].hypot(along[1])).max(1e-9);
            let (ax, ay) = (along[0] / n, along[1] / n);
            for (dx, dy, dz) in [(0.0, 0.0, l), (ax * l, ay * l, 0.0), (-ay * l, ax * l, 0.0)] {
                out.push(
                    Prim::new(
                        at,
                        vec![
                            [at[0] - dx, at[1] - dy, at[2] - dz],
                            [at[0] + dx, at[1] + dy, at[2] + dz],
                        ],
                        rgba(colors[0], alpha),
                        3.0,
                    )
                    .at(0, 0),
                );
            }
            return;
        }
        // A clamp: steel, its own shape or a short cylinder along the
        // wire.
        Some(o) if o.kind == ohl_line::ObjectKind::Clamp => {
            (clamp_mesh(o, at, along, subs), colors[1])
        }
        Some(o) => {
            body = true;
            (body_mesh(&o.shape, at, along), colors[2])
        }
    };
    if triangles.is_empty() {
        return;
    }
    let mut s = Prim::new(at, triangles, rgba(color, alpha), 0.0).at(0, 0);
    s.filled = true;
    out.push(s);
    if body {
        let mut d = Prim::new(at, vec![at], rgba(color, alpha), crate::lod::BODY_DOT_PX).at(1, 1);
        d.dot = true;
        out.push(d);
    }
}

/// The horizontal direction of a polyline's chord (along x without one).
pub fn chord_direction(points: &[[f64; 3]]) -> [f64; 2] {
    match (points.first(), points.last()) {
        (Some(a), Some(b)) => {
            let d = [b[0] - a[0], b[1] - a[1]];
            let n = d[0].hypot(d[1]);
            if n > 0.0 {
                [d[0] / n, d[1] / n]
            } else {
                [1.0, 0.0]
            }
        }
        _ => [1.0, 0.0],
    }
}

/// A body's solid as triangles facing out, at `at`, its axes along the
/// wire (`along`, horizontal), across the line and up: a sphere, a
/// cylinder (12 sides) along its axis, a box; given areas a sphere of the
/// wind area; a point nothing.
pub fn body_mesh(shape: &ohl_line::Shape, at: [f64; 3], along: [f64; 2]) -> Vec<[f64; 3]> {
    use std::f64::consts::PI;
    let e1 = [along[0], along[1], 0.0];
    let e2 = [-along[1], along[0], 0.0];
    let e3 = [0.0, 0.0, 1.0];
    let local =
        |x: f64, y: f64, z: f64| [0, 1, 2].map(|k| at[k] + x * e1[k] + y * e2[k] + z * e3[k]);
    let mut tris: Vec<[f64; 3]> = match *shape {
        ohl_line::Shape::Point => return Vec::new(),
        ohl_line::Shape::Sphere { diameter } if diameter > 0.0 => {
            return sphere(at, diameter / 2.0);
        }
        ohl_line::Shape::Areas { wind_area, .. } if wind_area > 0.0 => {
            return sphere(at, (wind_area / PI).sqrt());
        }
        ohl_line::Shape::Cylinder {
            diameter,
            length,
            axis,
        } if diameter > 0.0 && length > 0.0 => {
            let r = diameter / 2.0;
            let h = length / 2.0;
            // (along its axis, the two across it)
            let place = |s: f64, u: f64, v: f64| match axis {
                ohl_line::Axis::Along => local(s, u, v),
                ohl_line::Axis::Across => local(u, s, v),
                ohl_line::Axis::Vertical => local(u, v, s),
            };
            let n = 12;
            let ring = |s: f64, i: usize| {
                let a = 2.0 * PI * i as f64 / n as f64;
                place(s, r * a.cos(), r * a.sin())
            };
            let mut t = Vec::new();
            for i in 0..n {
                let (a0, a1, b0, b1) = (ring(-h, i), ring(-h, i + 1), ring(h, i), ring(h, i + 1));
                t.extend([a0, a1, b1, a0, b1, b0]);
                t.extend([place(-h, 0.0, 0.0), a1, a0]);
                t.extend([place(h, 0.0, 0.0), b0, b1]);
            }
            t
        }
        ohl_line::Shape::Box {
            along,
            across,
            height,
        } if along > 0.0 && across > 0.0 && height > 0.0 => {
            let (x, y, z) = (along / 2.0, across / 2.0, height / 2.0);
            let c = |i: usize| {
                local(
                    if i & 1 == 0 { -x } else { x },
                    if i & 2 == 0 { -y } else { y },
                    if i & 4 == 0 { -z } else { z },
                )
            };
            let faces = [
                [0, 1, 3, 2],
                [4, 6, 7, 5],
                [0, 4, 5, 1],
                [2, 3, 7, 6],
                [0, 2, 6, 4],
                [1, 5, 7, 3],
            ];
            faces
                .iter()
                .flat_map(|f| [c(f[0]), c(f[1]), c(f[2]), c(f[0]), c(f[2]), c(f[3])])
                .collect()
        }
        _ => return Vec::new(),
    };
    // Every triangle facing out (away from the centre).
    for t in tris.as_chunks_mut::<3>().0 {
        let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
        let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let m = [0, 1, 2].map(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0 - at[k]);
        if n[0] * m[0] + n[1] * m[1] + n[2] * m[2] < 0.0 {
            t.swap(1, 2);
        }
    }
    tris
}

/// Colour of span points.
const SPAN_POINT_COLOR: [u8; 3] = [90, 200, 230];

/// Colour of interphase spacers.
const SPACER_COLOR: [u8; 3] = [150, 210, 110];

/// Horizontal length of a span's polyline (its plan chord), m.
pub fn plan_length(points: &[[f64; 3]]) -> f64 {
    match (points.first(), points.last()) {
        (Some(a), Some(b)) => (b[0] - a[0]).hypot(b[1] - a[1]),
        _ => 0.0,
    }
}

/// The part of a span's polyline between two horizontal fractions.
pub fn between(points: &[[f64; 3]], from: f64, to: f64) -> Vec<[f64; 3]> {
    let (from, to) = (from.min(to).clamp(0.0, 1.0), from.max(to).clamp(0.0, 1.0));
    let (Some(a), Some(b)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    let chord = [b[0] - a[0], b[1] - a[1]];
    let len2 = chord[0] * chord[0] + chord[1] * chord[1];
    if len2 <= 0.0 {
        return Vec::new();
    }
    let t = |p: &[f64; 3]| ((p[0] - a[0]) * chord[0] + (p[1] - a[1]) * chord[1]) / len2;
    let mut out: Vec<[f64; 3]> = at_fraction(points, from).into_iter().collect();
    out.extend(
        points
            .iter()
            .filter(|p| (from..=to).contains(&t(p)))
            .copied(),
    );
    out.extend(at_fraction(points, to));
    out
}

/// The point of a span's polyline at a horizontal fraction of the way
/// between its ends (measured along their plan chord).
pub fn at_fraction(points: &[[f64; 3]], fraction: f64) -> Option<[f64; 3]> {
    let (a, b) = (points.first()?, points.last()?);
    let chord = [b[0] - a[0], b[1] - a[1]];
    let len2 = chord[0] * chord[0] + chord[1] * chord[1];
    if len2 <= 0.0 {
        return None;
    }
    let t = |p: &[f64; 3]| ((p[0] - a[0]) * chord[0] + (p[1] - a[1]) * chord[1]) / len2;
    points.windows(2).find_map(|w| {
        let (t0, t1) = (t(&w[0]), t(&w[1]));
        ((t0 - fraction) * (t1 - fraction) <= 0.0 && t1 != t0).then(|| {
            let u = (fraction - t0) / (t1 - t0);
            [0, 1, 2].map(|k| w[0][k] + u * (w[1][k] - w[0][k]))
        })
    })
}

/// A sphere of radius `r` around `c` as triangles (12 around, 8 from
/// pole to pole), counter-clockwise seen from outside.
pub fn sphere(c: [f64; 3], r: f64) -> Vec<[f64; 3]> {
    use std::f64::consts::PI;
    let (around, down) = (12, 8);
    let at = |i: usize, j: usize| {
        let (phi, theta) = (
            PI * j as f64 / down as f64,
            2.0 * PI * i as f64 / around as f64,
        );
        [
            c[0] + r * phi.sin() * theta.cos(),
            c[1] + r * phi.sin() * theta.sin(),
            c[2] + r * phi.cos(),
        ]
    };
    let mut out = Vec::with_capacity(around * down * 6);
    for j in 0..down {
        for i in 0..around {
            let [a, b, c, d] = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
            if j > 0 {
                out.extend([a, c, b]);
            }
            if j + 1 < down {
                out.extend([b, c, d]);
            }
        }
    }
    out
}

#[cfg(test)]
#[test]
fn spheres_face_out_and_are_lit_from_above() {
    let ball = sphere([0.0; 3], 1.0);
    for t in ball.as_chunks::<3>().0 {
        // Outward: the normal points away from the centre.
        let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
        let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let c = [0, 1, 2].map(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
        assert!((0..3).map(|k| n[k] * c[k]).sum::<f64>() > 0.0);
    }
    let top = ball
        .as_chunks::<3>()
        .0
        .iter()
        .find(|t| t.iter().all(|p| p[2] > 0.9))
        .unwrap();
    let bottom = ball
        .as_chunks::<3>()
        .0
        .iter()
        .find(|t| t.iter().all(|p| p[2] < -0.9))
        .unwrap();
    let c = [200, 100, 50, 255];
    assert!(lit(top, c)[0] > lit(bottom, c)[0]);
}

/// A clamp as drawn: without a shape a short cylinder along the wire; its
/// own cylinder across the line alone as given, on a bundle joining the
/// subconductors (a ring of bars on three); a rod's triangles face out.
#[cfg(test)]
#[test]
fn clamps_are_drawn_to_their_shape_and_across_a_bundle() {
    let extent = |t: &[[f64; 3]], k: usize| {
        let (lo, hi) = t.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[k]), hi.max(p[k]))
        });
        hi - lo
    };
    let at = [0.0, 0.0, 10.0];
    let along = [1.0, 0.0];
    // No shape: along (x here), its clamp length.
    let mut clamp = ohl_line::LineObject::clamp(20.0);
    clamp.clamp_length = Some(0.25);
    let plain = clamp_mesh(&clamp, at, along, &[]);
    assert!((extent(&plain, 0) - 0.25).abs() < 1e-9);
    // Its own cylinder across (y): its length.
    clamp.shape = ohl_line::Shape::Cylinder {
        diameter: 0.06,
        length: 0.3,
        axis: ohl_line::Axis::Across,
    };
    let own = clamp_mesh(&clamp, at, along, &[]);
    assert!((extent(&own, 1) - 0.3).abs() < 1e-9 && extent(&own, 0) < 0.07);
    // A 2-bundle 0.4 m apart: one bar between its subconductors.
    let line = ohl_line::Line::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap();
    let mut sec = line.sections[0].clone();
    (sec.bundle, sec.bundle_spacing, sec.bundle_rotation_deg) = (2, Some(0.4), None);
    let wire = [[-50.0, 0.0, 10.0], [50.0, 0.0, 10.0]];
    let subs = subconductors_at(&sec, &wire, at);
    assert_eq!(subs.len(), 2);
    let d = [0, 1, 2].map(|k| subs[1][k] - subs[0][k]);
    assert!(((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - 0.4).abs() < 1e-9);
    let bar = clamp_mesh(&clamp, at, along, &subs);
    assert!(
        (extent(&bar, 1) - 0.4).abs() < 1e-6 && extent(&bar, 0) < 0.07,
        "{}",
        extent(&bar, 1)
    );
    // A 3-bundle: three bars, a ring through its subconductors.
    sec.bundle = 3;
    let subs = subconductors_at(&sec, &wire, at);
    assert_eq!(clamp_mesh(&clamp, at, along, &subs).len(), 3 * bar.len());
    // A single wire: no subconductors, the cylinder as given.
    (sec.bundle, sec.bundle_spacing) = (1, None);
    assert!(subconductors_at(&sec, &wire, at).is_empty());
    // A rod's triangles face away from its axis.
    for t in rod_mesh([0.0; 3], [0.0, 0.0, 1.0], 0.2).as_chunks::<3>().0 {
        let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
        let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let c = [0, 1, 2].map(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0 - [0.0, 0.0, 0.5][k]);
        assert!((0..3).map(|k| n[k] * c[k]).sum::<f64>() > 0.0);
    }
}

/// A filled triangle's shade: its colour lit from the north-west, above,
/// on its outer side (counter-clockwise seen from outside); its shadow
/// side darker.
fn lit(t: &[[f64; 3]], color: [u8; 4]) -> [u8; 4] {
    let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
    let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    let light = [-0.5, 0.5, 0.75];
    let l = (0.5f64 * 0.5 + 0.5 * 0.5 + 0.75 * 0.75).sqrt();
    let cos = (0..3).map(|k| n[k] * light[k]).sum::<f64>() / (len * l);
    let f = 0.35 + 0.65 * cos.max(0.0);
    let [r, g, b, a] = color;
    let shade = |x: u8| (f64::from(x) * f).min(255.0) as u8;
    [shade(r), shade(g), shade(b), a]
}

/// The primitives of a layer drawn as primitives (structures, wires,
/// other cases, project lines).
pub fn prims(model: &Model, layer: Layer, case: usize) -> Vec<Prim> {
    let mut out = Vec::new();
    let line = &model.line;
    match layer {
        Layer::Structures => structures(&mut out, model, line, &model.cases, case, None),
        Layer::Wires | Layer::OtherCases => {
            wires(
                &mut out,
                model,
                line,
                &model.cases,
                case,
                layer == Layer::Wires,
                None,
            );
        }
        Layer::OtherLines => {
            // Each project line shown drawn as the working line is (strings
            // to their solved clamps, bundles, yokes, jumpers), in the same
            // weather cases, in its colour.
            // Only what differs from the line worked on, unless the
            // whole lines are asked for.
            for other in &model.other_lines {
                let tint = Some(other.color);
                let part;
                let line = if model.other_lines_whole {
                    &other.line
                } else {
                    part = other.differs.of_line(&other.line);
                    &part
                };
                structures(&mut out, model, line, &other.cases, case, tint);
                wires(&mut out, model, line, &other.cases, case, true, tint);
            }
        }
        _ => {}
    }
    out
}

/// Draws primitives at `level`, wires thinned to `per_span` points.
pub fn draw_prims<'a>(
    s: &mut Scene,
    prims: impl IntoIterator<Item = &'a Prim>,
    level: usize,
    per_span: usize,
) {
    for p in prims {
        if level < p.levels.0 || level > p.levels.1 {
            continue;
        }
        if p.filled {
            for t in p.points.as_chunks::<3>().0 {
                s.triangle(*t, [lit(t, p.color); 3]);
            }
        } else if p.dot {
            if let Some(at) = p.points.first() {
                s.point(*at, p.color, p.width);
            }
        } else if p.thinned && level > 0 {
            s.polyline(&thin(&p.points, per_span), p.color, p.width);
        } else {
            s.polyline(&p.points, p.color, p.width);
        }
    }
}

/// `n` (≥ 2) points of a polyline, evenly by index, both ends kept.
pub fn thin(points: &[[f64; 3]], n: usize) -> Vec<[f64; 3]> {
    let n = n.max(2);
    if points.len() <= n {
        return points.to_vec();
    }
    let last = points.len() - 1;
    (0..n)
        .map(|i| points[(i * last + (n - 1) / 2) / (n - 1)])
        .collect()
}

/// Shade of a triangle of another surface: its colour, lit like the
/// ground.
pub fn tinted_color(a: [f64; 3], b: [f64; 3], c: [f64; 3], tint: [u8; 3]) -> [u8; 4] {
    let g = ground_color(a, b, c);
    // The ground's light, as a factor of its base colour.
    let lit = f64::from(g[0]) / (120.0 * 1.3);
    rgba(tint.map(|x| (f64::from(x) * lit).min(255.0) as u8), 255)
}

/// Shade of a ground triangle (light from the north-west, above).
pub fn ground_color(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [u8; 4] {
    let light = {
        let l = [-0.5f64, 0.5, 0.75];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        l.map(|v| v / n)
    };
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let mut n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    n = n.map(|x| x / len);
    if n[2] < 0.0 {
        n = n.map(|x| -x);
    }
    let lit = 0.4 + 0.6 * (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    let col = [120.0, 108.0, 88.0].map(|x: f64| (x * lit * 1.3).min(255.0) as u8);
    rgba(col, 255)
}

/// Builds one layer of `model` for weather case `case`.
pub fn layer(model: &Model, layer: Layer, case: usize, selection: &Selection) -> Scene {
    let line = &model.line;
    let mut s = Scene::new(model.origin);
    match layer {
        Layer::Ground => {
            if let Some(g) = &model.ground {
                let (pts, tris) = g.mesh();
                for t in tris {
                    let [a, b, c] = t.map(|i| pts[i]);
                    s.triangle([a, b, c], [ground_color(a, b, c); 3]);
                }
            }
        }
        Layer::Surfaces => {
            for o in &model.other_surfaces {
                let pts = o.terrain.points();
                for t in o.terrain.triangles() {
                    let [a, b, c] = t.map(|i| pts[i as usize]);
                    s.triangle([a, b, c], [tinted_color(a, b, c, o.color); 3]);
                }
            }
        }
        Layer::Points => {
            for (q, c) in &model.points {
                if model.hidden_classes.contains(c) {
                    continue;
                }
                if let Some(col) = class_color(&model.scheme, *c) {
                    let p = [0, 1, 2].map(|k| model.origin[k] + f64::from(q[k]) / 1000.0);
                    s.point(p, rgba(col, 255), 2.0);
                }
            }
        }
        Layer::Alignment => {
            for r in line.alignment_views() {
                // Draped on the ground, sampled every 5 m.
                let c = r.centreline(line);
                let mut pts: Vec<[f64; 3]> = Vec::new();
                for w in c.windows(2) {
                    let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
                    let n = ((len / 5.0).ceil() as usize).max(1);
                    for i in 0..=n {
                        let t = i as f64 / n as f64;
                        let (x, y) = (
                            w[0][0] + (w[1][0] - w[0][0]) * t,
                            w[0][1] + (w[1][1] - w[0][1]) * t,
                        );
                        let z = model
                            .ground
                            .as_ref()
                            .and_then(|g| g.height(x, y))
                            .unwrap_or(w[0][2] + (w[1][2] - w[0][2]) * t);
                        pts.push([x, y, z + 0.3]);
                    }
                }
                s.polyline(&pts, [90, 200, 220, 200], 1.5);
            }
            // Every alignment point (those on no alignment too).
            for p in &line.alignment_points {
                s.point(
                    crate::selection::draped(model, p.position),
                    [90, 200, 220, 255],
                    8.0,
                );
            }
        }
        // The rigid links: solid rods from the joints to the points.
        Layer::RigidLinks => {
            for (_, _, a, b) in crate::link_tool::drawn_links(model) {
                s.polyline(&[a, b], crate::link_tool::RIGID, 4.0);
            }
        }
        // Full detail (the chunks draw these by level: crate::chunks).
        Layer::Structures | Layer::Wires | Layer::OtherCases | Layer::OtherLines => {
            draw_prims(&mut s, &prims(model, layer, case), 0, usize::MAX);
        }
        // Drawn from the clearance report ([`marks`]); orthophotos in plan
        // only.
        Layer::Clearances
        | Layer::Draft
        | Layer::Orthophotos
        | Layer::GrowIn
        | Layer::FallIn
        | Layer::Isoclearance
        | Layer::GroundLines
        | Layer::MagneticField
        | Layer::SafetyVolume => {}
        Layer::Drawings => crate::backgrounds::scene(model, &mut s),
        Layer::Selection => {
            let hi = [255, 255, 255, 255];
            // The selection and the others selected (Shift or Cmd/Ctrl+click).
            for selection in crate::selection::all_selected(selection, model) {
                match selection {
                    Selection::Structure(id) => {
                        if let Some(st) = line.structure(id) {
                            // Its wire points where the strings hold them in
                            // the case shown (as solved), else as given.
                            let pieces = crate::selection::string_pieces(model, case);
                            for w in &st.wires {
                                for p in wire_drawn(st, w, &pieces) {
                                    s.point(p, hi, 9.0);
                                }
                            }
                            s.point(st.position, hi, 12.0);
                        }
                    }
                    Selection::Span { section, span } => {
                        // In every case it is drawn in (both wind sides), the
                        // copy clicked (else the first case's) the strongest.
                        let cases = model.section_cases(section, case);
                        let strong = model
                            .selection_case
                            .filter(|k| cases.contains(k))
                            .unwrap_or(cases[0]);
                        for k in cases {
                            let Some(c) = model.cases.get(k) else {
                                continue;
                            };
                            for (side, solved) in c.drawn().enumerate() {
                                let Some(sp) = solved
                                    .shapes
                                    .iter()
                                    .find(|x| x.section == *section)
                                    .and_then(|x| x.spans.get(*span))
                                else {
                                    // Not solved in this case: its copy drawn
                                    // dashed in its place, highlighted too.
                                    if let Some(points) = model
                                        .unsolved_drawn(section, k, side)
                                        .and_then(|d| d.spans.get(*span).cloned())
                                        .filter(|p| p.len() > 1)
                                    {
                                        let main = k == strong && side == 0;
                                        s.polyline(&points, hi, if main { 5.0 } else { 2.5 });
                                    }
                                    continue;
                                };
                                let lines =
                                    match solved.bundles.spans.get(&(section.clone(), *span)) {
                                        Some(l) => l.clone(),
                                        None => vec![sp.points.clone()],
                                    };
                                let main = k == strong && side == 0;
                                let width = match (main, lines.len() > 1) {
                                    (true, true) => 3.0,
                                    (true, false) => 5.0,
                                    (false, true) => 1.5,
                                    (false, false) => 2.5,
                                };
                                for l in &lines {
                                    s.polyline(l, hi, width);
                                }
                            }
                        }
                    }
                    Selection::Jumper(id) => {
                        let cases = model.shown(case);
                        let strong = model
                            .selection_case
                            .filter(|k| cases.contains(k))
                            .unwrap_or(case);
                        for k in cases {
                            let Some(c) = model.cases.get(k) else {
                                continue;
                            };
                            for (side, solved) in c.drawn().enumerate() {
                                if let Some(j) = solved.jumpers.iter().find(|x| &x.jumper == id) {
                                    let (_, lines) = jumper_lines(line, j, &solved.bundles);
                                    let width = if k == strong && side == 0 { 3.0 } else { 1.5 };
                                    for l in &lines {
                                        s.polyline(l, hi, width);
                                    }
                                }
                            }
                        }
                    }
                    Selection::Attachment { structure, set } => {
                        // The strings holding it where they are drawn in every
                        // case drawn (as solved), else as given; the copy
                        // clicked (else the case shown) the strongest.
                        let strong = model.selection_case.unwrap_or(case);
                        if let Some(st) = line.structure(structure)
                            && let Some(w) = st.wire_at_set(*set)
                        {
                            for k in crate::selection::drawn_cases(model, case) {
                                if k == strong {
                                    continue;
                                }
                                let pieces = crate::selection::string_pieces(model, k);
                                for &i in &st.component(&w.label).strings {
                                    let x = &st.strings[i];
                                    for (top, bottom) in pieces
                                        .get(&(st.id.clone(), x.label.clone()))
                                        .into_iter()
                                        .flatten()
                                    {
                                        s.polyline(&[*top, *bottom], hi, 1.5);
                                    }
                                }
                                for p in wire_drawn(st, w, &pieces) {
                                    s.point(p, hi, 7.0);
                                }
                            }
                            let pieces = crate::selection::string_pieces(model, strong);
                            for &i in &st.component(&w.label).strings {
                                let x = &st.strings[i];
                                match pieces.get(&(st.id.clone(), x.label.clone())) {
                                    Some(drawn) => {
                                        for (top, bottom) in drawn {
                                            s.polyline(&[*top, *bottom], hi, 3.0);
                                            s.point(*top, hi, 9.0);
                                        }
                                    }
                                    None => {
                                        if let (Some(top), Some(bottom)) =
                                            (st.node(&x.top), st.wire(&x.wire))
                                        {
                                            s.polyline(&[top.position(), bottom.position], hi, 3.0);
                                            s.point(top.position(), hi, 9.0);
                                        }
                                    }
                                }
                            }
                            for p in wire_drawn(st, w, &pieces) {
                                s.point(p, hi, 11.0);
                            }
                        }
                    }
                    Selection::String { structure, label } => {
                        // Where it is drawn in every case drawn (both wind
                        // sides), else as given; the copies in the case clicked
                        // (else the one shown) the strongest.
                        let strong = model.selection_case.unwrap_or(case);
                        for k in crate::selection::drawn_cases(model, case) {
                            if k == strong {
                                continue;
                            }
                            for (a, b) in crate::selection::string_pieces(model, k)
                                .remove(&(structure.clone(), label.clone()))
                                .unwrap_or_default()
                            {
                                s.polyline(&[a, b], hi, 2.0);
                            }
                        }
                        let mut drawn = crate::selection::string_pieces(model, strong)
                            .remove(&(structure.clone(), label.clone()))
                            .unwrap_or_default();
                        if drawn.is_empty()
                            && let Some(st) = line.structure(structure)
                            && let Some(x) = st.string(label)
                            && let (Some(top), Some(w)) = (st.node(&x.top), st.wire(&x.wire))
                        {
                            drawn.push((top.position(), w.position));
                        }
                        for (a, b) in drawn {
                            s.polyline(&[a, b], hi, 4.0);
                            s.point(a, hi, 9.0);
                            s.point(b, hi, 9.0);
                        }
                    }
                    Selection::Point { structure, label } => {
                        // Its rigid link (an attachment point at an offset).
                        for (_, _, a, b) in crate::link_tool::drawn_links(model)
                            .into_iter()
                            .filter(|l| &l.0 == structure && &l.1 == label)
                        {
                            s.polyline(&[a, b], hi, 5.0);
                            s.point(a, hi, 9.0);
                        }
                        // A wire point: where it hangs in every case drawn.
                        let strong = model.selection_case.unwrap_or(case);
                        if let Some(st) = line.structure(structure)
                            && let Some(w) = st.wire(label)
                        {
                            for k in crate::selection::drawn_cases(model, case) {
                                let pieces = crate::selection::string_pieces(model, k);
                                for p in wire_drawn(st, w, &pieces) {
                                    s.point(p, hi, if k == strong { 12.0 } else { 8.0 });
                                }
                            }
                        } else if let Some(p) = selection.focus(model, case) {
                            s.point(p, hi, 12.0);
                        }
                    }
                    Selection::SpanPoint { section, label } => {
                        // Every case its section is drawn in (both wind sides).
                        let strong = model.selection_case.unwrap_or(case);
                        for (k, solved) in drawn_copies(model, section, case) {
                            let Some(p) = solved
                                .shapes
                                .iter()
                                .find(|x| x.section == *section)
                                .and_then(|x| x.span_points.iter().find(|(l, _)| l == label))
                                .map(|(_, p)| *p)
                            else {
                                continue;
                            };
                            s.point(p, hi, if k == strong { 12.0 } else { 8.0 });
                        }
                    }
                    Selection::Spread { section, index } => {
                        // Its stretch in every case its section is drawn in
                        // (both wind sides), the copy clicked the strongest.
                        let strong = model.selection_case.unwrap_or(case);
                        let def = line
                            .sections
                            .iter()
                            .find(|x| x.id == *section)
                            .map(|x| line.section_with_fractions(x));
                        for (k, solved) in drawn_copies(model, section, case) {
                            let Some(stretch) = def.as_ref().and_then(|def| {
                                let shape = solved.shapes.iter().find(|x| x.section == *section)?;
                                spreads_drawn(def, shape)
                                    .into_iter()
                                    .find(|(i, _)| i == index)
                                    .map(|(_, s)| s)
                            }) else {
                                continue;
                            };
                            let main = k == strong;
                            s.polyline(&stretch, hi, if main { 6.0 } else { 3.0 });
                            for p in [stretch[0], stretch[stretch.len() - 1]] {
                                s.point(p, hi, if main { 9.0 } else { 6.0 });
                            }
                        }
                    }
                    Selection::JumperPoint { jumper, label } => {
                        // Every case drawn (both wind sides).
                        let strong = model.selection_case.unwrap_or(case);
                        for k in crate::selection::drawn_cases(model, case) {
                            let Some(c) = model.cases.get(k) else {
                                continue;
                            };
                            for solved in c.drawn() {
                                if let Some(p) = solved
                                    .jumpers
                                    .iter()
                                    .find(|j| &j.jumper == jumper)
                                    .and_then(|j| {
                                        j.point_positions.iter().find(|(l, _)| l == label)
                                    })
                                    .map(|(_, p)| *p)
                                {
                                    s.point(p, hi, if k == strong { 12.0 } else { 8.0 });
                                }
                            }
                        }
                    }
                    Selection::Spacer(id) => {
                        let strong = model.selection_case.unwrap_or(case);
                        let held = line
                            .spacers
                            .iter()
                            .find(|x| &x.id == id)
                            .map(|x| x.a.section.clone());
                        for (k, solved) in held
                            .iter()
                            .flat_map(|section| drawn_copies(model, section, case))
                        {
                            let Some(x) = solved
                                .shapes
                                .iter()
                                .flat_map(|x| &x.spacers)
                                .find(|x| &x.id == id)
                            else {
                                continue;
                            };
                            let [a, b] = x.ends;
                            let main = k == strong;
                            s.polyline(&[a, b], hi, if main { 5.0 } else { 2.5 });
                            s.point(a, hi, if main { 9.0 } else { 6.0 });
                            s.point(b, hi, if main { 9.0 } else { 6.0 });
                        }
                    }
                    Selection::AlignmentPoint { id } => {
                        if let Some(p) = crate::selection::alignment_point(model, id) {
                            s.point(p, hi, 12.0);
                            s.polyline(&[p, [p[0], p[1], p[2] + 30.0]], hi, 1.5);
                        }
                    }
                    Selection::LidarPoint(_) | Selection::SurveyPoint(_) => {
                        if let Some(p) = crate::selection::position(model, selection) {
                            s.point(p, hi, 10.0);
                        }
                    }
                    Selection::None => {}
                }
            }
        }
    }
    s
}

/// Clearance violations as drawn: a red line from the wire to the target,
/// a dot on the wire.
/// Dimmed while the check that found them is out of date (`stale`).
pub fn marks(origin: [f64; 3], marks: &[([f64; 3], [f64; 3])], stale: bool) -> Scene {
    let mut s = Scene::new(origin);
    let c = |rgba| crate::revision::dimmed(rgba, stale);
    for (at, target) in marks {
        s.polyline(&[*at, *target], c([255, 60, 50, 255]), 2.5);
        s.point(*at, c([255, 60, 50, 255]), 9.0);
        s.point(*target, c([255, 160, 60, 255]), 6.0);
    }
    s
}

/// Cyan of grow-in points, orange of danger trees (also in the plan and
/// the profile).
pub const GROW_IN: [u8; 3] = [60, 220, 230];
/// See [`GROW_IN`].
pub const FALL_IN: [u8; 3] = [255, 150, 40];

/// What Checks › Vegetation found, on layer `layer`: every grow-in point
/// (a dot) and every danger tree (its trunk from base to top), in the
/// user's two colours; isoclearance lines in their colours; dimmed while the report is out of date.
pub fn vegetation(origin: [f64; 3], overlay: &crate::vegetation::Overlay, layer: Layer) -> Scene {
    let mut s = Scene::new(origin);
    let c = |rgb: [u8; 3]| crate::revision::dimmed([rgb[0], rgb[1], rgb[2], 255], overlay.stale);
    match layer {
        // Every point found, one batch each (no line to the wire).
        Layer::GrowIn => {
            let col = c(overlay.colors[0]);
            for p in &overlay.grow {
                s.point(*p, col, 6.0);
            }
        }
        Layer::FallIn => {
            let col = c(overlay.colors[1]);
            for (top, base) in &overlay.fall {
                s.polyline(&[*base, *top], col, 2.0);
                s.point(*top, col, 6.0);
            }
        }
        Layer::Isoclearance => {
            for (_, rgb, points) in &overlay.lines {
                // Lifted a little off the ground.
                let pts: Vec<[f64; 3]> = points.iter().map(|p| [p[0], p[1], p[2] + 0.2]).collect();
                s.polyline(&pts, c(*rgb), 1.5);
            }
        }
        _ => {}
    }
    s
}

/// A wire being strung: its points joined, yellow; and what a replacement
/// would remove, red.
pub fn draft(origin: [f64; 3], points: &[[f64; 3]], removed: &[Vec<[f64; 3]>]) -> Scene {
    let mut s = Scene::new(origin);
    // What a replacement would remove (Replace structure), red.
    for l in removed {
        s.polyline(l, [255, 60, 60, 255], 4.0);
    }
    if points.len() > 1 {
        s.polyline(points, [255, 220, 60, 255], 3.0);
    }
    for p in points {
        s.point(*p, [255, 220, 60, 255], 11.0);
    }
    s
}

/// Where wire point `w` of `st` is drawn: the ends of the strings to it as
/// solved (`pieces`: the case shown and its mirror), else where it is
/// given (a wire point without a string, or a case not solved yet).
pub(crate) fn wire_drawn(
    st: &ohl_line::Structure,
    w: &ohl_line::WirePoint,
    pieces: &crate::selection::StringPieces,
) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = st
        .strings
        .iter()
        .filter(|x| x.wire == w.label)
        .filter_map(|x| pieces.get(&(st.id.clone(), x.label.clone())))
        .flatten()
        .map(|(_, bottom)| *bottom)
        .collect();
    out.dedup();
    if out.is_empty() {
        out.push(w.position);
    }
    out
}
