// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Weight and quantities of one lattice structure (joint details stage
//! 4.6, 2026-11-19): what it is built from, counted and weighed, as plain
//! data — the members, the gusset plates, the splices' covers, the bolts
//! and the welds — and its bill of materials ([`StructureBom`]).
//!
//! The structure application builds the [`Part`]s from the structure file
//! and its joint details (`ohl_structure_link::quantities`); this module
//! holds the rules of the weighing, so that they are checked by hand on
//! invented parts ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/structure-weight.md)):
//!
//! - a **plate** or **cover**: its outline's area × its thickness × the
//!   steel's density ([`plate_mass`]); the holes **not deducted** in the
//!   heavy count (on the safe side where the weight is unfavourable),
//!   deducted in the light count ([`hole_area`]);
//! - a **fillet weld**: its section, a triangle of throat \\( a \\) in the
//!   corner between the plates, × its length × the density
//!   ([`fillet_section`]);
//! - a **bolt** without a mass in its library: its shank, head and nut
//!   weighed from their sizes ([`bolt_mass_estimate`]).
//!
//! Every part has two masses: the **heavy count** (`mass`: each doubt on
//! the heavy side — for a weight that loads the structure) and the
//! **light count** (`light`: each doubt on the light side — for a weight
//! that holds it, uplift and overturning).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ohl_core::text::csv_field;
use ohl_core::units::STANDARD_GRAVITY;
use serde::Serialize;

/// The weight of steel per volume, N/m³: SIA 261:2014, Annexe A, Table 28
/// ("Charges volumiques moyennes des matériaux": "Acier 78,5 kN/m³"). The
/// steel of a part whose own material gives no density (a plate, a bolt).
pub const STEEL_UNIT_WEIGHT: f64 = 78.5e3;

/// [`STEEL_UNIT_WEIGHT`] as a density, kg/m³ (÷ the standard gravity:
/// 8 004.8 kg/m³, heavier than the 7 850 kg/m³ libraries often give — on
/// the safe side for a weight that loads the structure).
pub fn steel_density() -> f64 {
    STEEL_UNIT_WEIGHT / STANDARD_GRAVITY
}

/// The most a fillet's corner is taken open in the heavy count, degrees
/// (an assumption: plates meeting flatter than this take a butt weld, not
/// a fillet).
pub const FILLET_ANGLE_MAX_DEG: f64 = 135.0;

/// The least, in the light count, degrees (the same assumption).
pub const FILLET_ANGLE_MIN_DEG: f64 = 45.0;

/// The largest throat of a fillet weld whose size is not given, as a
/// share of the thinner plate's thickness: SIA 263:2013 6.3.6.3 (the
/// throat "ne dépassera pas 0,7 fois l'épaisseur de la pièce la plus
/// mince"). The heavy count of a weld without a size.
pub const FILLET_THROAT_MAX: f64 = 0.7;

/// What a part is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    /// A member (its section × its length).
    Member,
    /// A gusset plate.
    Plate,
    /// A splice's cover.
    Cover,
    /// Bolts of one kind (with their nuts and washers as the bolt library
    /// gives them).
    Bolt,
    /// A weld.
    Weld,
}

impl PartKind {
    /// Every kind, in the bill's order.
    pub const ALL: [PartKind; 5] = [
        PartKind::Member,
        PartKind::Plate,
        PartKind::Cover,
        PartKind::Bolt,
        PartKind::Weld,
    ];

    /// Name in the bill and the exports.
    pub fn as_str(self) -> &'static str {
        match self {
            PartKind::Member => "member",
            PartKind::Plate => "plate",
            PartKind::Cover => "cover",
            PartKind::Bolt => "bolt",
            PartKind::Weld => "weld",
        }
    }

    /// Plural, for the totals.
    pub fn plural(self) -> &'static str {
        match self {
            PartKind::Member => "members",
            PartKind::Plate => "plates",
            PartKind::Cover => "covers",
            PartKind::Bolt => "bolts",
            PartKind::Weld => "welds",
        }
    }

    /// The unit of its quantity: m (members, welds), m² (plates, covers),
    /// pcs (bolts).
    pub fn unit(self) -> &'static str {
        match self {
            PartKind::Member | PartKind::Weld => "m",
            PartKind::Plate | PartKind::Cover => "m2",
            PartKind::Bolt => "pcs",
        }
    }
}

/// One part counted: a member, a plate, a cover, the bolts of one end of
/// one kind, a weld.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    /// What it is.
    pub kind: PartKind,
    /// Its line in the bill: the section (`L90x9`), `plate 10 mm S355`,
    /// `M16 8.8 grip 24 mm`, `fillet a 6 mm`…
    pub item: String,
    /// Its id (a member's, a plate's; bolts and welds: the member end or
    /// the seam).
    pub id: String,
    /// The joint it is at (none: a member).
    pub joint: Option<String>,
    /// Pieces.
    pub count: usize,
    /// Its quantity in [`PartKind::unit`]: a length, an area, a count.
    pub quantity: f64,
    /// Heavy count, kg.
    pub mass: f64,
    /// Light count, kg.
    pub light: f64,
    /// Laid out by the automatic joint layout (its bolts provisional or
    /// its part made by it).
    pub automatic: bool,
    /// Its mass estimated (a bolt without a mass in its library, a weld
    /// without a size, a plate outline worked out).
    pub estimated: bool,
}

/// One line of the bill: the parts of one kind and item together.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BomRow {
    /// What they are.
    pub kind: PartKind,
    /// The item.
    pub item: String,
    /// Pieces.
    pub count: usize,
    /// Their quantity, in [`PartKind::unit`].
    pub quantity: f64,
    /// Heavy count, kg.
    pub mass: f64,
    /// Light count, kg.
    pub light: f64,
    /// Of `count`, the pieces laid out by the automatic layout.
    pub automatic: usize,
    /// Of `count`, the pieces whose mass is estimated.
    pub estimated: usize,
}

/// The parts of one kind together.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindTotal {
    /// What they are.
    pub kind: PartKind,
    /// Pieces.
    pub count: usize,
    /// Heavy count, kg.
    pub mass: f64,
    /// Light count, kg.
    pub light: f64,
}

/// The weight and quantities of one structure, and its bill of
/// materials.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct StructureBom {
    /// The structure's id.
    pub structure: String,
    /// Every part counted.
    pub parts: Vec<Part>,
    /// The bill: per kind and item, in [`PartKind::ALL`]'s order, then by
    /// item.
    pub rows: Vec<BomRow>,
    /// Per kind (those with parts).
    pub totals: Vec<KindTotal>,
    /// Everything, heavy count, kg.
    pub mass: f64,
    /// Everything, light count, kg.
    pub light: f64,
    /// The members' steel as the finite-element model weighs it, kg (their
    /// sections × their lengths between the joints): what the dead-load
    /// factor multiplies, the base of [`Self::factor`]. [`Self::new`]
    /// takes the members' heavy count; the caller may set the model's.
    pub members_mass: f64,
    /// The joints not defined, with why ("D1: no connection", "left for a
    /// hand detail"): their parts are not counted.
    pub undefined: Vec<(String, String)>,
    /// What was assumed, estimated or left out, in words; "NOT ON THE
    /// SAFE SIDE" where it is not.
    pub notes: Vec<String>,
}

impl StructureBom {
    /// The bill of `parts` of structure `structure`; `undefined`: the
    /// joints not defined, with why; `notes`: what the counting assumed.
    pub fn new(
        structure: &str,
        parts: Vec<Part>,
        undefined: Vec<(String, String)>,
        notes: Vec<String>,
    ) -> Self {
        let mut rows: BTreeMap<(PartKind, String), BomRow> = BTreeMap::new();
        for p in &parts {
            let r = rows
                .entry((p.kind, p.item.clone()))
                .or_insert_with(|| BomRow {
                    kind: p.kind,
                    item: p.item.clone(),
                    count: 0,
                    quantity: 0.0,
                    mass: 0.0,
                    light: 0.0,
                    automatic: 0,
                    estimated: 0,
                });
            r.count += p.count;
            r.quantity += p.quantity;
            r.mass += p.mass;
            r.light += p.light;
            if p.automatic {
                r.automatic += p.count;
            }
            if p.estimated {
                r.estimated += p.count;
            }
        }
        let rows: Vec<BomRow> = rows.into_values().collect();
        let totals: Vec<KindTotal> = PartKind::ALL
            .iter()
            .filter_map(|&k| {
                let of: Vec<&BomRow> = rows.iter().filter(|r| r.kind == k).collect();
                (!of.is_empty()).then(|| KindTotal {
                    kind: k,
                    count: of.iter().map(|r| r.count).sum(),
                    mass: of.iter().map(|r| r.mass).sum(),
                    light: of.iter().map(|r| r.light).sum(),
                })
            })
            .collect();
        let members_mass = totals
            .iter()
            .find(|t| t.kind == PartKind::Member)
            .map_or(0.0, |t| t.mass);
        Self {
            structure: structure.to_owned(),
            mass: totals.iter().map(|t| t.mass).sum(),
            light: totals.iter().map(|t| t.light).sum(),
            members_mass,
            rows,
            totals,
            parts,
            undefined,
            notes,
        }
    }

    /// Every joint defined: no joint left out of the count.
    pub fn every_joint_defined(&self) -> bool {
        self.undefined.is_empty()
    }

    /// The heavy count over the members' steel: the factor on the members'
    /// self-weight that gives the counted mass (none: no members' steel).
    pub fn factor(&self) -> Option<f64> {
        (self.members_mass > 0.0).then(|| self.mass / self.members_mass)
    }

    /// The light count over the members' steel.
    pub fn light_factor(&self) -> Option<f64> {
        (self.members_mass > 0.0).then(|| self.light / self.members_mass)
    }

    /// The total of `kind`, if it has parts.
    pub fn total(&self, kind: PartKind) -> Option<&KindTotal> {
        self.totals.iter().find(|t| t.kind == kind)
    }

    /// The bill as a text table, the totals, the joints not defined and
    /// the notes.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Weight and quantities: {}", self.structure);
        let head = [
            "kind", "item", "count", "quantity", "unit", "mass kg", "light kg",
        ]
        .map(str::to_owned);
        let rows: Vec<[String; 7]> = self
            .rows
            .iter()
            .map(|r| {
                [
                    r.kind.as_str().to_owned(),
                    row_item(r),
                    r.count.to_string(),
                    quantity_text(r.quantity, r.kind),
                    r.kind.unit().to_owned(),
                    format!("{:.1}", r.mass),
                    format!("{:.1}", r.light),
                ]
            })
            .collect();
        let mut width = [0usize; 7];
        for r in std::iter::once(&head).chain(&rows) {
            for (k, c) in r.iter().enumerate() {
                width[k] = width[k].max(c.chars().count());
            }
        }
        let line = |out: &mut String, r: &[String; 7]| {
            let cells: Vec<String> = (0..7)
                .map(|k| {
                    if k == 2 || k == 3 || k >= 5 {
                        format!("{:>w$}", r[k], w = width[k])
                    } else {
                        format!("{:<w$}", r[k], w = width[k])
                    }
                })
                .collect();
            let _ = writeln!(out, "  {}", cells.join("  ").trim_end());
        };
        line(&mut out, &head);
        for r in &rows {
            line(&mut out, r);
        }
        for t in &self.totals {
            let _ = writeln!(
                out,
                "  {}: {} pcs, {:.1} kg (light {:.1} kg)",
                t.kind.plural(),
                t.count,
                t.mass,
                t.light
            );
        }
        let _ = writeln!(
            out,
            "Total: {:.1} kg (light count {:.1} kg){}",
            self.mass,
            self.light,
            self.factor()
                .map_or(String::new(), |f| format!(", {f:.3} × the members' steel"))
        );
        if !self.undefined.is_empty() {
            let _ = writeln!(
                out,
                "Joints not defined ({}), their parts not counted:",
                self.undefined.len()
            );
            for (j, why) in &self.undefined {
                let _ = writeln!(out, "  {j}: {why}");
            }
        }
        for n in &self.notes {
            let _ = writeln!(out, "note: {n}");
        }
        out
    }

    /// CSV: `kind,item,count,quantity,unit,mass_kg,light_kg,automatic,estimated`,
    /// one row per line of the bill, a total row per kind
    /// (`total <kind>`), the total (`total`), then the joints not defined
    /// (`undefined`, the item `<joint>: <why>`) and the notes (`note`, the
    /// text as the item).
    pub fn csv(&self) -> String {
        let mut out =
            String::from("kind,item,count,quantity,unit,mass_kg,light_kg,automatic,estimated\n");
        for r in &self.rows {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{}",
                r.kind.as_str(),
                csv_field(&r.item),
                r.count,
                round(r.quantity, 6),
                r.kind.unit(),
                round(r.mass, 3),
                round(r.light, 3),
                r.automatic,
                r.estimated
            );
        }
        for t in &self.totals {
            let _ = writeln!(
                out,
                "total {},,{},,,{},{},,",
                t.kind.as_str(),
                t.count,
                round(t.mass, 3),
                round(t.light, 3)
            );
        }
        let _ = writeln!(
            out,
            "total,,,,,{},{},,",
            round(self.mass, 3),
            round(self.light, 3)
        );
        for (j, why) in &self.undefined {
            let _ = writeln!(
                out,
                "undefined,{},,,,,,,",
                csv_field(&format!("{j}: {why}"))
            );
        }
        for n in &self.notes {
            let _ = writeln!(out, "note,{},,,,,,,", csv_field(n));
        }
        out
    }
}

/// A row's item with how many of its pieces are automatic or estimated.
pub fn row_item(r: &BomRow) -> String {
    let mut marks = Vec::new();
    if r.automatic > 0 {
        marks.push(if r.automatic == r.count {
            "automatic".to_owned()
        } else {
            format!("{} automatic", r.automatic)
        });
    }
    if r.estimated > 0 {
        marks.push(if r.estimated == r.count {
            "estimated".to_owned()
        } else {
            format!("{} estimated", r.estimated)
        });
    }
    if marks.is_empty() {
        r.item.clone()
    } else {
        format!("{} ({})", r.item, marks.join(", "))
    }
}

fn round(x: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (x * k).round() / k
}

fn quantity_text(q: f64, kind: PartKind) -> String {
    match kind {
        PartKind::Bolt => format!("{q:.0}"),
        PartKind::Plate | PartKind::Cover => format!("{q:.4}"),
        PartKind::Member | PartKind::Weld => format!("{q:.3}"),
    }
}

/// The area of a flat polygon in 3D (its points in order), m² (Newell's
/// method: half the length of the sum of the edges' cross products).
pub fn polygon_area(points: &[[f64; 3]]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = [0.0; 3];
    for k in 0..n {
        let a = points[k];
        let b = points[(k + 1) % n];
        s[0] += (a[1] - b[1]) * (a[2] + b[2]);
        s[1] += (a[2] - b[2]) * (a[0] + b[0]);
        s[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    0.5 * (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt()
}

/// The mass of a plate of `area` (m²), `thickness` (m) and `density`
/// (kg/m³), kg.
pub fn plate_mass(area: f64, thickness: f64, density: f64) -> f64 {
    area.max(0.0) * thickness.max(0.0) * density.max(0.0)
}

/// The area of a round hole of diameter `d` (m), m².
pub fn hole_area(d: f64) -> f64 {
    std::f64::consts::PI * d * d / 4.0
}

/// The section of a fillet weld of throat `a` (m) between plates meeting
/// at `angle` degrees (their faces from the seam; 90 for plates square to
/// each other), m²: (heavy, light). The fillet is a triangle of height
/// `a` in the corner, isosceles (equal legs, SIA 263:2013 6.3.2.6, Figure
/// 34); its apex angle φ gives the section \\( a^2 \tan(φ/2) \\) (a² at
/// 90°). The data do not say on which side of the seam the fillet lies:
/// the heavy count takes the more open corner (φ the larger of `angle`
/// and 180° − `angle`, at most [`FILLET_ANGLE_MAX_DEG`]), the light count
/// the closer one (at least [`FILLET_ANGLE_MIN_DEG`]).
pub fn fillet_section(a: f64, angle: f64) -> (f64, f64) {
    let angle = if angle.is_finite() {
        angle.clamp(0.0, 180.0)
    } else {
        90.0
    };
    let open = angle.max(180.0 - angle).min(FILLET_ANGLE_MAX_DEG);
    let close = angle.min(180.0 - angle).max(FILLET_ANGLE_MIN_DEG);
    let s = |phi: f64| a * a * (phi.to_radians() / 2.0).tan();
    (s(open), s(close))
}

/// The mass of a bolt from its sizes, kg, when its library gives none:
/// the shank (a cylinder of the nominal diameter `d` over `shank`, the
/// length under the head — the grip, the nut and what stands beyond it),
/// the head and the nut (hexagonal prisms `flats` across the flats, `head`
/// and `nut` high) in steel of `density` (kg/m³). The nut's hole is not
/// deducted and the threads not reduced (on the heavy side); washers are
/// not in it (the data give none).
pub fn bolt_mass_estimate(
    d: f64,
    shank: f64,
    flats: f64,
    head: f64,
    nut: f64,
    density: f64,
) -> f64 {
    let hexagon = 3f64.sqrt() / 2.0 * flats * flats;
    (hole_area(d) * shank.max(0.0) + hexagon * (head.max(0.0) + nut.max(0.0))) * density
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: PartKind, item: &str, count: usize, q: f64, mass: f64, light: f64) -> Part {
        Part {
            kind,
            item: item.into(),
            id: item.into(),
            joint: None,
            count,
            quantity: q,
            mass,
            light,
            automatic: false,
            estimated: false,
        }
    }

    /// SIA 261 Table 28: 78.5 kN/m³ ÷ 9.80665 m/s² = 8 004.8 kg/m³.
    #[test]
    fn steel_density_from_sia_261() {
        assert!(
            (steel_density() - 8004.77).abs() < 0.01,
            "{}",
            steel_density()
        );
    }

    /// A plate 300 × 200 mm, 10 mm thick, in steel of 7 850 kg/m³:
    /// 0.06 m² × 0.01 m × 7 850 = 4.71 kg; laid in any plane.
    #[test]
    fn plate_by_hand() {
        let flat = [
            [0.0, 0.0, 0.0],
            [0.3, 0.0, 0.0],
            [0.3, 0.2, 0.0],
            [0.0, 0.2, 0.0],
        ];
        assert!((polygon_area(&flat) - 0.06).abs() < 1e-12);
        // The same plate standing in a tilted plane.
        let (c, s) = (0.6f64, 0.8f64);
        let tilted: Vec<[f64; 3]> = flat
            .iter()
            .map(|p| [p[0], p[1] * c, p[1] * s + 5.0])
            .collect();
        assert!((polygon_area(&tilted) - 0.06).abs() < 1e-12);
        assert!((plate_mass(0.06, 0.01, 7850.0) - 4.71).abs() < 1e-12);
        // Four holes of 17.5 mm: 4 × π 0.0175²/4 = 962.1 mm²; light count
        // (0.06 − 0.000 962 1) × 0.01 × 7 850 = 4.6345 kg.
        let holes = 4.0 * hole_area(0.0175);
        assert!((holes - 962.11e-6).abs() < 1e-8, "{holes}");
        let light = plate_mass(0.06 - holes, 0.01, 7850.0);
        assert!((light - 4.6345).abs() < 1e-4, "{light}");
    }

    /// A fillet of throat 6 mm between plates at 90°: 36 mm² both ways;
    /// at 120°: heavy a² tan 60° = 62.35 mm², light a² tan 30° =
    /// 20.78 mm²; flatter than 135° held at it.
    #[test]
    fn fillet_by_hand() {
        let (h, l) = fillet_section(0.006, 90.0);
        assert!((h - 36e-6).abs() < 1e-12 && (l - 36e-6).abs() < 1e-12);
        let (h, l) = fillet_section(0.006, 120.0);
        assert!((h - 62.354e-6).abs() < 1e-9, "{h}");
        assert!((l - 20.785e-6).abs() < 1e-9, "{l}");
        // The other side of the same seam: the same.
        assert_eq!(fillet_section(0.006, 60.0), fillet_section(0.006, 120.0));
        let (h, l) = fillet_section(0.006, 170.0);
        assert!((h - 0.006f64.powi(2) * 67.5f64.to_radians().tan()).abs() < 1e-12);
        assert!((l - 0.006f64.powi(2) * 22.5f64.to_radians().tan()).abs() < 1e-12);
        // 220 mm of it at 90° in steel of 7 850 kg/m³, both sides:
        // 2 × 36e-6 × 0.22 × 7 850 = 0.124 34 kg.
        let m: f64 = 2.0 * 36e-6 * 0.22 * 7850.0;
        assert!((m - 0.12434).abs() < 1e-5);
    }

    /// An M16 bolt, 60 mm under its head, head and nut 24 mm across the
    /// flats, 10 and 13 mm high, steel of 7 850 kg/m³: shank π 16²/4 × 60
    /// = 12 063.7 mm³, hexagons √3/2 × 24² = 498.83 mm² × 23 mm =
    /// 11 473.1 mm³; 23 536.8 mm³ × 7 850 = 0.184 76 kg.
    #[test]
    fn bolt_by_hand() {
        let m = bolt_mass_estimate(0.016, 0.060, 0.024, 0.010, 0.013, 7850.0);
        assert!((m - 0.18476).abs() < 1e-5, "{m}");
    }

    /// The bill adds the parts per kind and item; the totals per kind; the
    /// factor is the heavy count over the members' steel.
    #[test]
    fn bill_adds_up() {
        let mut parts = vec![
            part(PartKind::Member, "L90x9", 1, 3.0, 36.6, 36.6),
            part(PartKind::Member, "L90x9", 1, 2.0, 24.4, 24.4),
            part(PartKind::Member, "L50x5", 1, 1.5, 5.6, 5.6),
            part(PartKind::Plate, "plate 10 mm", 1, 0.06, 4.71, 4.63),
            part(PartKind::Bolt, "M16 8.8 grip 20 mm", 4, 4.0, 0.8, 0.8),
            part(PartKind::Bolt, "M16 8.8 grip 20 mm", 2, 2.0, 0.4, 0.4),
            part(PartKind::Weld, "fillet a 6 mm", 1, 0.22, 0.124, 0.062),
        ];
        parts[5].automatic = true;
        let b = StructureBom::new("T", parts, Vec::new(), Vec::new());
        assert_eq!(b.rows.len(), 5);
        let bolts = b.rows.iter().find(|r| r.kind == PartKind::Bolt).unwrap();
        assert_eq!((bolts.count, bolts.automatic), (6, 2));
        assert!((bolts.mass - 1.2).abs() < 1e-12);
        assert_eq!(row_item(bolts), "M16 8.8 grip 20 mm (2 automatic)");
        assert!((b.members_mass - 66.6).abs() < 1e-9);
        assert!((b.mass - (66.6 + 4.71 + 1.2 + 0.124)).abs() < 1e-9);
        assert!((b.light - (66.6 + 4.63 + 1.2 + 0.062)).abs() < 1e-9);
        assert!((b.factor().unwrap() - b.mass / 66.6).abs() < 1e-12);
        assert!(b.every_joint_defined());
        let csv = b.csv();
        assert!(csv.starts_with("kind,item,count,quantity,unit,mass_kg"));
        assert!(csv.contains("total bolt,,6,,,1.2,1.2,,"), "{csv}");
        assert!(b.text().contains("bolts: 6 pcs, 1.2 kg"), "{}", b.text());
        let missing = StructureBom::new(
            "T",
            Vec::new(),
            vec![("N1".into(), "D1: no connection".into())],
            Vec::new(),
        );
        assert!(!missing.every_joint_defined());
        assert_eq!(missing.factor(), None);
        assert!(missing.csv().contains("undefined,N1: D1: no connection"));
    }
}
