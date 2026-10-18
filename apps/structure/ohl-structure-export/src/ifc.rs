// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! IFC (ISO 16739, STEP clear text) of a level 3 structure, in the
//! structure's axes, metres, newtons, pascals.
//!
//! **IFC4** (ADD2 TC1), in one file:
//!
//! - the **members** as `IfcMember` placed with their local z along the
//!   member and x along its local y, drawn as an axis line and, when the
//!   section has a profile, an `IfcExtrudedAreaSolid` of the profile
//!   (angles `IfcLShapeProfileDef`, channels `IfcUShapeProfileDef`, I
//!   sections `IfcIShapeProfileDef`, rectangular tubes
//!   `IfcRectangleHollowProfileDef`, tubes `IfcCircleHollowProfileDef`,
//!   round bars `IfcCircleProfileDef`, polygonal tubes
//!   `IfcArbitraryProfileDefWithVoids`), its centroid on the member's
//!   axis; the material and profile through an `IfcMaterialProfileSetUsage`
//!   (cardinal point 10, the centroid);
//! - the **structural analysis model** (`IfcStructuralAnalysisModel`):
//!   joints as `IfcStructuralPointConnection` (supports and springs as
//!   `IfcBoundaryNodeCondition`), members as `IfcStructuralCurveMember`
//!   (local z given by `Axis`), connected by
//!   `IfcRelConnectsStructuralMember`; every load case an
//!   `IfcStructuralLoadCase` (its self-weight factor as
//!   `SelfWeightCoefficients`) with its loads as `IfcStructuralPointAction`
//!   (at a joint, or at a point along a member) and
//!   `IfcStructuralCurveAction` (spread along a member), global axes.
//!
//! **IFC2x3** (TC1, Coordination View 2.0): the members only, with their
//! profiles and materials, in a site, building and storey — the form
//! steel-detailing programs convert into their own parts.
//!
//! GlobalIds come from the structure's id and each object's id, so an
//! export of the same structure gives the same ids.

use ohl_sections::Shape;
use ohl_structure_link::{ExportKind, ExportMember, ExportSection, ExportStructure, LoadPlace};

use crate::step::{Step, opt_string, real, string};

/// The IFC schema written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Schema {
    /// IFC4 ADD2 TC1: members and the structural analysis model.
    #[default]
    Ifc4,
    /// IFC2x3 TC1, Coordination View 2.0: members only.
    Ifc2x3,
}

/// What an IFC file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct IfcOptions {
    /// Schema.
    pub schema: Schema,
    /// The members as `IfcMember` with their profiles.
    pub members: bool,
    /// The structural analysis model with the load cases (IFC4 only).
    pub analysis: bool,
    /// Time of the export, seconds since 1970 (UTC); 0 keeps files of
    /// the same structure byte for byte the same.
    pub time: i64,
}

impl Default for IfcOptions {
    fn default() -> Self {
        Self {
            schema: Schema::Ifc4,
            members: true,
            analysis: true,
            time: 0,
        }
    }
}

/// `YYYY-MM-DDThh:mm:ss` of seconds since 1970 (UTC).
pub fn iso_time(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let s = t.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

fn add3(a: [f64; 3], b: [f64; 3], f: f64) -> [f64; 3] {
    [a[0] + f * b[0], a[1] + f * b[1], a[2] + f * b[2]]
}

/// A length that must be positive when given: `$` for 0.
fn positive(x: f64) -> String {
    if x > 0.0 { real(x) } else { "$".into() }
}

/// The vertices of a regular polygon with a side at the bottom, its
/// centre at the origin, apothem `a` (counter-clockwise).
fn polygon(sides: u32, a: f64) -> Vec<[f64; 2]> {
    let n = f64::from(sides);
    let r = a / (std::f64::consts::PI / n).cos();
    (0..sides)
        .map(|k| {
            let t = -std::f64::consts::FRAC_PI_2
                + std::f64::consts::PI / n
                + 2.0 * std::f64::consts::PI * f64::from(k) / n;
            [r * t.cos(), r * t.sin()]
        })
        .collect()
}

/// The IFC entity name and its attributes after `ProfileType, ProfileName,
/// Position` for a parameterised profile, or a whole arbitrary profile.
enum Profile {
    Parameterized(&'static str, String),
    Polygonal(u32, f64, f64),
}

fn profile_kind(shape: &Shape, schema: Schema) -> Option<Profile> {
    let ifc4 = schema == Schema::Ifc4;
    Some(match *shape {
        Shape::EqualAngle { h, t, r } | Shape::UnequalAngle { h, t, r, b: _ } => {
            let b = match *shape {
                Shape::UnequalAngle { b, .. } => b,
                _ => h,
            };
            let tail = if ifc4 { "$,$" } else { "$,$,$,$" };
            Profile::Parameterized(
                "IFCLSHAPEPROFILEDEF",
                format!("{},{},{},{},{tail}", real(h), real(b), real(t), positive(r)),
            )
        }
        Shape::Channel { h, b, tw, tf, r } => {
            let tail = if ifc4 { "$,$" } else { "$,$,$" };
            Profile::Parameterized(
                "IFCUSHAPEPROFILEDEF",
                format!(
                    "{},{},{},{},{},{tail}",
                    real(h),
                    real(b),
                    real(tw),
                    real(tf),
                    positive(r)
                ),
            )
        }
        Shape::ISection { h, b, tw, tf, r } => Profile::Parameterized(
            "IFCISHAPEPROFILEDEF",
            format!(
                "{},{},{},{},{}{}",
                real(b),
                real(h),
                real(tw),
                real(tf),
                positive(r),
                if ifc4 { ",$,$" } else { "" }
            ),
        ),
        Shape::RectangularHollow { h, b, t } => Profile::Parameterized(
            "IFCRECTANGLEHOLLOWPROFILEDEF",
            format!("{},{},{},$,$", real(b), real(h), real(t)),
        ),
        Shape::CircularHollow { d, t } => Profile::Parameterized(
            "IFCCIRCLEHOLLOWPROFILEDEF",
            format!("{},{}", real(d / 2.0), real(t)),
        ),
        Shape::RoundBar { d } => Profile::Parameterized("IFCCIRCLEPROFILEDEF", real(d / 2.0)),
        Shape::PolygonalHollow { sides, d, t } => Profile::Polygonal(sides, d, t),
        Shape::General { .. } => return None,
    })
}

/// The IFC entity a section's profile maps to (none: no geometry).
pub fn profile_entity(shape: &Shape) -> Option<&'static str> {
    profile_kind(shape, Schema::Ifc4).map(|p| match p {
        Profile::Parameterized(e, _) => e,
        Profile::Polygonal(..) => "IFCARBITRARYPROFILEDEFWITHVOIDS",
    })
}

/// A section's profile with its centroid at the origin; none when it has
/// no geometry (IFC4 then writes a profile by name only, `IfcProfileDef`).
fn profile(f: &mut Step, s: &ExportSection, schema: Schema) -> Option<String> {
    let kind = s.shape.as_ref().and_then(|sh| profile_kind(sh, schema));
    let name = string(&s.id);
    match kind {
        Some(Profile::Parameterized(entity, args)) => {
            let at = f.point2([-s.centroid[0], -s.centroid[1]]);
            let x = f.add("IFCDIRECTION", "(1.,0.)");
            let place = f.add("IFCAXIS2PLACEMENT2D", &format!("{at},{x}"));
            Some(f.add(entity, &format!(".AREA.,{name},{place},{args}")))
        }
        Some(Profile::Polygonal(sides, d, t)) => {
            let mut ring = |a: f64, reverse: bool| {
                let mut pts = polygon(sides, a);
                if reverse {
                    pts.reverse();
                }
                pts.push(pts[0]);
                let refs: Vec<String> = pts.iter().map(|p| f.point2(*p)).collect();
                f.add("IFCPOLYLINE", &format!("({})", refs.join(",")))
            };
            let outer = ring(d / 2.0, false);
            let inner = ring(d / 2.0 - t, true);
            Some(f.add(
                "IFCARBITRARYPROFILEDEFWITHVOIDS",
                &format!(".AREA.,{name},{outer},({inner})"),
            ))
        }
        None if schema == Schema::Ifc4 => Some(f.add("IFCPROFILEDEF", &format!(".AREA.,{name}"))),
        None => None,
    }
}

/// Whether a section is drawn as a solid (its profile has geometry).
fn solid(s: &ExportSection) -> bool {
    s.shape
        .as_ref()
        .is_some_and(|sh| profile_kind(sh, Schema::Ifc4).is_some())
}

fn label(t: &str) -> String {
    format!("IFCLABEL({})", string(t))
}

/// A property set on objects.
fn properties(
    f: &mut Step,
    oh: &str,
    key: &str,
    objects: &[String],
    name: &str,
    values: &[(&str, String)],
) {
    let props: Vec<String> = values
        .iter()
        .map(|(n, v)| f.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{v},$", string(n))))
        .collect();
    let g = f.guid(&format!("pset:{key}"));
    let set = f.add(
        "IFCPROPERTYSET",
        &format!("{g},{oh},{},$,({})", string(name), props.join(",")),
    );
    let g = f.guid(&format!("pset-rel:{key}"));
    f.add(
        "IFCRELDEFINESBYPROPERTIES",
        &format!("{g},{oh},$,$,({}),{set}", objects.join(",")),
    );
}

/// The IFC file of `s`.
pub fn ifc(s: &ExportStructure, o: &IfcOptions) -> Result<String, String> {
    if o.schema == Schema::Ifc2x3 && o.analysis {
        return Err("IFC2x3 export writes the members only (the analysis model is IFC4)".into());
    }
    if !o.members && !o.analysis {
        return Err("nothing to write: neither members nor the analysis model".into());
    }
    let ifc4 = o.schema == Schema::Ifc4;
    let mut f = Step::new(&s.id);
    // Contexts, units, owner history (IFC2x3).
    let origin = f.point([0.0; 3]);
    let up = f.direction([0.0, 0.0, 1.0]);
    let east = f.direction([1.0, 0.0, 0.0]);
    let world = f.add("IFCAXIS2PLACEMENT3D", &format!("{origin},{up},{east}"));
    let context = f.add(
        "IFCGEOMETRICREPRESENTATIONCONTEXT",
        &format!("$,'Model',3,1.E-05,{world},$"),
    );
    let body = f.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        &format!("'Body','Model',*,*,*,*,{context},$,.MODEL_VIEW.,$"),
    );
    let axis_ctx = f.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        &format!("'Axis','Model',*,*,*,*,{context},$,.GRAPH_VIEW.,$"),
    );
    let si = |f: &mut Step, kind: &str, prefix: &str, name: &str| {
        f.add("IFCSIUNIT", &format!("*,.{kind}.,{prefix},.{name}."))
    };
    let metre = si(&mut f, "LENGTHUNIT", "$", "METRE");
    let radian = si(&mut f, "PLANEANGLEUNIT", "$", "RADIAN");
    let newton = si(&mut f, "FORCEUNIT", "$", "NEWTON");
    let mut units = vec![metre.clone(), radian.clone(), newton.clone()];
    if ifc4 {
        units.push(si(&mut f, "PRESSUREUNIT", "$", "PASCAL"));
        let kg = si(&mut f, "MASSUNIT", ".KILO.", "GRAM");
        units.push(kg.clone());
        units.push(si(&mut f, "AREAUNIT", "$", "SQUARE_METRE"));
        units.push(si(&mut f, "VOLUMEUNIT", "$", "CUBIC_METRE"));
        let mut derived = |f: &mut Step, kind: &str, elements: &[(&str, i32)]| {
            let e: Vec<String> = elements
                .iter()
                .map(|(u, x)| f.add("IFCDERIVEDUNITELEMENT", &format!("{u},{x}")))
                .collect();
            let u = f.add("IFCDERIVEDUNIT", &format!("({}),.{kind}.,$", e.join(",")));
            units.push(u);
        };
        derived(
            &mut f,
            "MODULUSOFELASTICITYUNIT",
            &[(&newton, 1), (&metre, -2)],
        );
        derived(&mut f, "SHEARMODULUSUNIT", &[(&newton, 1), (&metre, -2)]);
        derived(&mut f, "MASSDENSITYUNIT", &[(&kg, 1), (&metre, -3)]);
        derived(&mut f, "MOMENTOFINERTIAUNIT", &[(&metre, 4)]);
        derived(&mut f, "SECTIONMODULUSUNIT", &[(&metre, 3)]);
        derived(&mut f, "LINEARFORCEUNIT", &[(&newton, 1), (&metre, -1)]);
        derived(&mut f, "LINEARSTIFFNESSUNIT", &[(&newton, 1), (&metre, -1)]);
        derived(
            &mut f,
            "ROTATIONALSTIFFNESSUNIT",
            &[(&newton, 1), (&metre, 1), (&radian, -1)],
        );
    }
    let units = f.add("IFCUNITASSIGNMENT", &format!("({})", units.join(",")));
    let oh = if ifc4 {
        "$".to_owned()
    } else {
        let person = f.add("IFCPERSON", "$,$,$,$,$,$,$,$");
        let org = f.add("IFCORGANIZATION", "$,'OpenOHL',$,$,$");
        let user = f.add("IFCPERSONANDORGANIZATION", &format!("{person},{org},$"));
        let app = f.add(
            "IFCAPPLICATION",
            &format!(
                "{org},{},'OpenOHL','ohl-structure-export'",
                string(env!("CARGO_PKG_VERSION"))
            ),
        );
        f.add(
            "IFCOWNERHISTORY",
            &format!("{user},{app},$,.NOCHANGE.,$,$,$,{}", o.time),
        )
    };
    let g = f.guid("project");
    let project = f.add(
        "IFCPROJECT",
        &format!(
            "{g},{oh},{},{},$,$,$,({context}),{units}",
            string(&s.name),
            opt_string(&s.description)
        ),
    );
    let site_place = f.add("IFCLOCALPLACEMENT", &format!("$,{world}"));
    let g = f.guid("site");
    let site = f.add(
        "IFCSITE",
        &format!(
            "{g},{oh},{},$,$,{site_place},$,$,.ELEMENT.,$,$,$,$,$",
            string(&s.id)
        ),
    );
    let g = f.guid("project-site");
    f.add(
        "IFCRELAGGREGATES",
        &format!("{g},{oh},$,$,{project},({site})"),
    );
    // Where the members are contained (IFC2x3 CV 2.0: a storey).
    let (container, member_place) = if ifc4 {
        (site.clone(), site_place.clone())
    } else {
        let bplace = f.add("IFCLOCALPLACEMENT", &format!("{site_place},{world}"));
        let g = f.guid("building");
        let building = f.add(
            "IFCBUILDING",
            &format!(
                "{g},{oh},{},$,$,{bplace},$,$,.ELEMENT.,$,$,$",
                string(&s.id)
            ),
        );
        let splace = f.add("IFCLOCALPLACEMENT", &format!("{bplace},{world}"));
        let g = f.guid("storey");
        let storey = f.add(
            "IFCBUILDINGSTOREY",
            &format!("{g},{oh},'Structure',$,$,{splace},$,$,.ELEMENT.,0.0"),
        );
        let g = f.guid("site-building");
        f.add(
            "IFCRELAGGREGATES",
            &format!("{g},{oh},$,$,{site},({building})"),
        );
        let g = f.guid("building-storey");
        f.add(
            "IFCRELAGGREGATES",
            &format!("{g},{oh},$,$,{building},({storey})"),
        );
        (storey, splace)
    };

    // Materials and profiles.
    let materials: Vec<String> = s
        .materials
        .iter()
        .map(|m| {
            if ifc4 {
                f.add(
                    "IFCMATERIAL",
                    &format!(
                        "{},$,{}",
                        string(&m.name),
                        string(&crate::material_kind(m).to_ascii_lowercase())
                    ),
                )
            } else {
                f.add("IFCMATERIAL", &string(&m.name))
            }
        })
        .collect();
    if ifc4 {
        for (m, e) in s.materials.iter().zip(&materials) {
            let p = |f: &mut Step, n: &str, v: String| {
                f.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{v},$", string(n)))
            };
            let mech = [
                p(
                    &mut f,
                    "YoungModulus",
                    format!("IFCMODULUSOFELASTICITYMEASURE({})", real(m.elasticity)),
                ),
                p(
                    &mut f,
                    "ShearModulus",
                    format!("IFCMODULUSOFELASTICITYMEASURE({})", real(m.shear_modulus)),
                ),
                p(
                    &mut f,
                    "PoissonRatio",
                    format!("IFCPOSITIVERATIOMEASURE({})", real(m.poisson().max(1e-6))),
                ),
            ];
            f.add(
                "IFCMATERIALPROPERTIES",
                &format!("'Pset_MaterialMechanical',$,({}),{e}", mech.join(",")),
            );
            if m.density > 0.0 {
                let d = p(
                    &mut f,
                    "MassDensity",
                    format!("IFCMASSDENSITYMEASURE({})", real(m.density)),
                );
                f.add(
                    "IFCMATERIALPROPERTIES",
                    &format!("'Pset_MaterialCommon',$,({d}),{e}"),
                );
            }
            if let Some(y) = m.yield_strength {
                let d = p(
                    &mut f,
                    "YieldStress",
                    format!("IFCPRESSUREMEASURE({})", real(y)),
                );
                f.add(
                    "IFCMATERIALPROPERTIES",
                    &format!("'Pset_MaterialSteel',$,({d}),{e}"),
                );
            }
        }
    }
    let profiles: Vec<Option<String>> = s
        .sections
        .iter()
        .map(|sec| profile(&mut f, sec, o.schema))
        .collect();
    // IFC4: a profile set per section, its properties.
    let usages: Vec<Option<String>> = if ifc4 {
        s.sections
            .iter()
            .zip(&profiles)
            .map(|(sec, p)| {
                let p = p.as_ref()?;
                let values = [
                    (
                        "CrossSectionArea",
                        format!("IFCAREAMEASURE({})", real(sec.area)),
                    ),
                    (
                        "TorsionalConstantX",
                        format!("IFCMOMENTOFINERTIAMEASURE({})", real(sec.it)),
                    ),
                ];
                let props: Vec<String> = values
                    .iter()
                    .map(|(n, v)| {
                        f.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{v},$", string(n)))
                    })
                    .collect();
                f.add(
                    "IFCPROFILEPROPERTIES",
                    &format!("'Pset_ProfileMechanical',$,({}),{p}", props.join(",")),
                );
                let own = [
                    (
                        "SecondMomentLocalY",
                        format!("IFCMOMENTOFINERTIAMEASURE({})", real(sec.iy)),
                    ),
                    (
                        "SecondMomentLocalZ",
                        format!("IFCMOMENTOFINERTIAMEASURE({})", real(sec.iz)),
                    ),
                ];
                let props: Vec<String> = own
                    .iter()
                    .map(|(n, v)| {
                        f.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{v},$", string(n)))
                    })
                    .collect();
                f.add(
                    "IFCPROFILEPROPERTIES",
                    &format!("'OHL_Section',$,({}),{p}", props.join(",")),
                );
                let mp = f.add(
                    "IFCMATERIALPROFILE",
                    &format!("{},$,{},{p},$,$", string(&sec.id), materials[sec.material]),
                );
                let set = f.add(
                    "IFCMATERIALPROFILESET",
                    &format!("{},$,({mp}),$", string(&sec.name)),
                );
                Some(f.add("IFCMATERIALPROFILESETUSAGE", &format!("{set},10,$")))
            })
            .collect()
    } else {
        vec![None; s.sections.len()]
    };
    // What each section's usage (IFC4) or material (IFC2x3) goes to.
    let mut by_section: Vec<Vec<String>> = vec![Vec::new(); s.sections.len()];

    // Physical members.
    let mut physical: Vec<String> = Vec::new();
    if o.members {
        let identity = f.add("IFCAXIS2PLACEMENT3D", &format!("{origin},$,$"));
        let along = f.direction([0.0, 0.0, 1.0]);
        for m in &s.members {
            let e = member(
                &mut f,
                s,
                m,
                &oh,
                &member_place,
                &body,
                &axis_ctx,
                &identity,
                &along,
                profiles[m.section]
                    .as_deref()
                    .filter(|_| solid(&s.sections[m.section])),
                ifc4,
            );
            by_section[m.section].push(e.clone());
            physical.push(e);
        }
        let g = f.guid("contained");
        f.add(
            "IFCRELCONTAINEDINSPATIALSTRUCTURE",
            &format!("{g},{oh},$,$,({}),{container}", physical.join(",")),
        );
    }

    // The structural analysis model.
    if o.analysis {
        let reference = f.add(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
            &format!("'Reference','Model',*,*,*,*,{context},$,.GRAPH_VIEW.,$"),
        );
        let shared = f.add("IFCLOCALPLACEMENT", &format!("$,{world}"));
        let topology = |f: &mut Step, kind: &str, item: &str| {
            let rep = f.add(
                "IFCTOPOLOGYREPRESENTATION",
                &format!("{reference},'Reference','{kind}',({item})"),
            );
            f.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,({rep})"))
        };
        let vertices: Vec<String> = s
            .nodes
            .iter()
            .map(|n| {
                let p = f.point(n.point);
                f.add("IFCVERTEXPOINT", &p)
            })
            .collect();
        let mut connections = Vec::new();
        for (n, v) in s.nodes.iter().zip(&vertices) {
            let condition = if n.is_support() {
                let dof = |k: usize, stiffness: &str| {
                    if n.fixed[k] {
                        "IFCBOOLEAN(.T.)".to_owned()
                    } else if n.spring[k] != 0.0 {
                        format!("{stiffness}({})", real(n.spring[k]))
                    } else {
                        "IFCBOOLEAN(.F.)".to_owned()
                    }
                };
                let values: Vec<String> = (0..6)
                    .map(|k| {
                        dof(
                            k,
                            if k < 3 {
                                "IFCLINEARSTIFFNESSMEASURE"
                            } else {
                                "IFCROTATIONALSTIFFNESSMEASURE"
                            },
                        )
                    })
                    .collect();
                f.add(
                    "IFCBOUNDARYNODECONDITION",
                    &format!("{},{}", string(&n.id), values.join(",")),
                )
            } else {
                "$".to_owned()
            };
            let shape = topology(&mut f, "Vertex", v);
            let g = f.guid(&format!("connection:{}", n.id));
            connections.push(f.add(
                "IFCSTRUCTURALPOINTCONNECTION",
                &format!("{g},$,{},$,$,{shared},{shape},{condition},$", string(&n.id)),
            ));
        }
        let mut curves = Vec::new();
        for (k, m) in s.members.iter().enumerate() {
            let edge = f.add(
                "IFCEDGE",
                &format!("{},{}", vertices[m.nodes[0]], vertices[m.nodes[1]]),
            );
            let shape = topology(&mut f, "Edge", &edge);
            let axis = f.direction(m.axes[2]);
            let kind = match m.kind {
                ExportKind::Truss => "PIN_JOINED_MEMBER",
                ExportKind::Frame => "RIGID_JOINED_MEMBER",
                ExportKind::Cable => "CABLE",
                ExportKind::Tension => "TENSION_MEMBER",
            };
            let g = f.guid(&format!("curve:{}", m.id));
            let c = f.add(
                "IFCSTRUCTURALCURVEMEMBER",
                &format!(
                    "{g},$,{},$,$,{shared},{shape},.{kind}.,{axis}",
                    string(&m.id)
                ),
            );
            for (end, n) in m.nodes.iter().enumerate() {
                let g = f.guid(&format!("connects:{}:{end}", m.id));
                f.add(
                    "IFCRELCONNECTSSTRUCTURALMEMBER",
                    &format!("{g},$,$,$,{c},{},$,$,$,$", connections[*n]),
                );
            }
            if let Some(p) = physical.get(k) {
                let g = f.guid(&format!("represents:{}", m.id));
                f.add("IFCRELASSIGNSTOPRODUCT", &format!("{g},$,$,$,({c}),$,{p}"));
            }
            by_section[m.section].push(c.clone());
            curves.push(c);
        }
        // Load cases and their loads.
        let mut cases = Vec::new();
        let mut actions_of = Vec::new();
        for case in &s.load_cases {
            let sw = if case.self_weight != 0.0 {
                format!("(0.,0.,{})", real(-case.self_weight))
            } else {
                "$".into()
            };
            let g = f.guid(&format!("case:{}", case.name));
            let c = f.add(
                "IFCSTRUCTURALLOADCASE",
                &format!(
                    "{g},$,{},{},$,.LOAD_CASE.,.NOTDEFINED.,.NOTDEFINED.,$,$,{sw}",
                    string(&case.name),
                    opt_string(&case.description)
                ),
            );
            let mut actions = Vec::new();
            for (k, l) in case.loads.iter().enumerate() {
                let key = format!("action:{}:{k}", case.name);
                let g = f.guid(&key);
                let force = |f: &mut Step, entity: &str| {
                    f.add(
                        entity,
                        &format!(
                            "{},{},{},{},$,$,$",
                            string(&l.label),
                            real(l.force[0]),
                            real(l.force[1]),
                            real(l.force[2])
                        ),
                    )
                };
                let (action, on) = match l.place {
                    LoadPlace::Node(n) => {
                        let load = force(&mut f, "IFCSTRUCTURALLOADSINGLEFORCE");
                        (
                            f.add(
                                "IFCSTRUCTURALPOINTACTION",
                                &format!(
                                    "{g},$,{},$,$,$,$,{load},.GLOBAL_COORDS.,$",
                                    string(&l.label)
                                ),
                            ),
                            connections[n].clone(),
                        )
                    }
                    LoadPlace::OnMember { member, at } => {
                        let m = &s.members[member];
                        let p = add3(s.nodes[m.nodes[0]].point, m.axes[0], at * m.length);
                        let p = f.point(p);
                        let v = f.add("IFCVERTEXPOINT", &p);
                        let shape = topology(&mut f, "Vertex", &v);
                        let load = force(&mut f, "IFCSTRUCTURALLOADSINGLEFORCE");
                        (
                            f.add(
                                "IFCSTRUCTURALPOINTACTION",
                                &format!(
                                    "{g},$,{},$,$,{shared},{shape},{load},.GLOBAL_COORDS.,$",
                                    string(&l.label)
                                ),
                            ),
                            curves[member].clone(),
                        )
                    }
                    LoadPlace::AlongMember(member) => {
                        let load = force(&mut f, "IFCSTRUCTURALLOADLINEARFORCE");
                        (
                            f.add(
                                "IFCSTRUCTURALCURVEACTION",
                                &format!(
                                    "{g},$,{},$,$,$,$,{load},.GLOBAL_COORDS.,$,.TRUE_LENGTH.,.CONST.",
                                    string(&l.label)
                                ),
                            ),
                            curves[member].clone(),
                        )
                    }
                };
                let g = f.guid(&format!("{key}:on"));
                f.add(
                    "IFCRELCONNECTSSTRUCTURALACTIVITY",
                    &format!("{g},$,$,$,{on},{action}"),
                );
                actions.push(action);
            }
            cases.push(c.clone());
            actions_of.push((case.name.clone(), c, actions));
        }
        for (name, c, actions) in actions_of {
            if actions.is_empty() {
                continue;
            }
            let g = f.guid(&format!("case-loads:{name}"));
            f.add(
                "IFCRELASSIGNSTOGROUP",
                &format!("{g},$,$,$,({}),$,{c}", actions.join(",")),
            );
        }
        let loaded = if cases.is_empty() {
            "$".to_owned()
        } else {
            format!("({})", cases.join(","))
        };
        let g = f.guid("analysis-model");
        let model = f.add(
            "IFCSTRUCTURALANALYSISMODEL",
            &format!(
                "{g},$,{},{},$,.LOADING_3D.,$,{loaded},$,{shared}",
                string(&format!("{} analysis model", s.id)),
                opt_string(&s.name)
            ),
        );
        let items: Vec<String> = connections.iter().chain(&curves).cloned().collect();
        let g = f.guid("analysis-items");
        f.add(
            "IFCRELASSIGNSTOGROUP",
            &format!("{g},$,$,$,({}),$,{model}", items.join(",")),
        );
        let g = f.guid("analysis-site");
        f.add(
            "IFCRELSERVICESBUILDINGS",
            &format!("{g},$,$,$,{model},({site})"),
        );
    }

    // Materials onto what uses them.
    for (k, objects) in by_section.iter().enumerate() {
        if objects.is_empty() {
            continue;
        }
        let relating = match &usages[k] {
            Some(u) => u.clone(),
            None => materials[s.sections[k].material].clone(),
        };
        let g = f.guid(&format!("material:{}", s.sections[k].id));
        f.add(
            "IFCRELASSOCIATESMATERIAL",
            &format!("{g},{oh},$,$,({}),{relating}", objects.join(",")),
        );
    }

    let view = match (ifc4, o.analysis, o.members) {
        (false, _, _) => "CoordinationView_V2.0",
        (true, true, _) => "StructuralAnalysisView",
        (true, false, _) => "ReferenceView_V1.2",
    };
    Ok(f.file(
        &format!("ViewDefinition [{view}]"),
        &format!("{}.ifc", s.id),
        &iso_time(o.time),
        if ifc4 { "IFC4" } else { "IFC2X3" },
    ))
}

/// One `IfcMember` with its placement, axis and solid, and its property
/// set; returns its reference.
#[allow(clippy::too_many_arguments)]
fn member(
    f: &mut Step,
    s: &ExportStructure,
    m: &ExportMember,
    oh: &str,
    container_place: &str,
    body: &str,
    axis_ctx: &str,
    identity: &str,
    along: &str,
    profile: Option<&str>,
    ifc4: bool,
) -> String {
    let start = f.point(s.nodes[m.nodes[0]].point);
    let z = f.direction(m.axes[0]);
    let x = f.direction(m.axes[1]);
    let axes = f.add("IFCAXIS2PLACEMENT3D", &format!("{start},{z},{x}"));
    let place = f.add("IFCLOCALPLACEMENT", &format!("{container_place},{axes}"));
    let a = f.point([0.0; 3]);
    let b = f.point([0.0, 0.0, m.length]);
    let line = f.add("IFCPOLYLINE", &format!("({a},{b})"));
    let mut reps = vec![f.add(
        "IFCSHAPEREPRESENTATION",
        &format!("{axis_ctx},'Axis','Curve3D',({line})"),
    )];
    if let Some(p) = profile {
        let solid = f.add(
            "IFCEXTRUDEDAREASOLID",
            &format!("{p},{identity},{along},{}", real(m.length)),
        );
        reps.push(f.add(
            "IFCSHAPEREPRESENTATION",
            &format!("{body},'Body','SweptSolid',({solid})"),
        ));
    }
    let shape = f.add(
        "IFCPRODUCTDEFINITIONSHAPE",
        &format!("$,$,({})", reps.join(",")),
    );
    let g = f.guid(&format!("member:{}", m.id));
    let section = &s.sections[m.section];
    let e = f.add(
        "IFCMEMBER",
        &format!(
            "{g},{oh},{},$,{},{place},{shape},{}{}",
            string(&m.id),
            string(m.kind.as_str()),
            string(&m.id),
            if ifc4 { ",.MEMBER." } else { "" }
        ),
    );
    properties(
        f,
        oh,
        &format!("member:{}", m.id),
        std::slice::from_ref(&e),
        "OHL_Member",
        &[
            ("Section", label(&section.id)),
            ("Kind", label(m.kind.as_str())),
            ("EffectiveLengthFactor", format!("IFCREAL({})", real(m.k))),
            (
                "Pretension",
                format!("IFCFORCEMEASURE({})", real(m.pretension)),
            ),
            ("From", label(&s.nodes[m.nodes[0]].id)),
            ("To", label(&s.nodes[m.nodes[1]].id)),
        ],
    );
    e
}
