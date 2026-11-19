// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The bill of materials of a structure (joint details stage 4.6): its
//! weight and quantities (`ohl_structure_link::structure_quantities`) as
//! CSV (`bill_of_materials.csv` among the CSV tables), as an XLSX
//! workbook ([`workbook`]: the bill, every part, the joints not defined
//! and the notes) and as a section of the structure report
//! ([`report_section`]).

use std::fmt::Write as _;

use ohl_bom::structure::{PartKind, StructureBom, row_item};

use crate::xlsx::{Cell, Sheet};

/// The bill's sheets: `Bill of materials` (per kind and item, the totals
/// after), `Parts` (every part counted), `Not defined` (the joints left
/// out, with why) and `Notes`.
pub fn sheets(b: &StructureBom) -> Vec<Sheet> {
    let mut bill = Sheet::new(
        "Bill of materials",
        &[
            "kind",
            "item",
            "count",
            "quantity",
            "unit",
            "mass_kg",
            "light_kg",
            "automatic",
            "estimated",
        ],
    );
    for r in &b.rows {
        bill.rows.push(vec![
            r.kind.as_str().into(),
            r.item.clone().into(),
            (r.count as f64).into(),
            r.quantity.into(),
            r.kind.unit().into(),
            r.mass.into(),
            r.light.into(),
            (r.automatic as f64).into(),
            (r.estimated as f64).into(),
        ]);
    }
    for t in &b.totals {
        bill.rows.push(vec![
            format!("total {}", t.kind.as_str()).into(),
            Cell::Empty,
            (t.count as f64).into(),
            Cell::Empty,
            Cell::Empty,
            t.mass.into(),
            t.light.into(),
            Cell::Empty,
            Cell::Empty,
        ]);
    }
    bill.rows.push(vec![
        "total".into(),
        Cell::Empty,
        Cell::Empty,
        Cell::Empty,
        Cell::Empty,
        b.mass.into(),
        b.light.into(),
        Cell::Empty,
        Cell::Empty,
    ]);
    let mut parts = Sheet::new(
        "Parts",
        &[
            "kind",
            "item",
            "id",
            "joint",
            "count",
            "quantity",
            "unit",
            "mass_kg",
            "light_kg",
            "automatic",
            "estimated",
        ],
    );
    for p in &b.parts {
        parts.rows.push(vec![
            p.kind.as_str().into(),
            p.item.clone().into(),
            p.id.clone().into(),
            p.joint.clone().into(),
            (p.count as f64).into(),
            p.quantity.into(),
            p.kind.unit().into(),
            p.mass.into(),
            p.light.into(),
            if p.automatic { "yes" } else { "" }.into(),
            if p.estimated { "yes" } else { "" }.into(),
        ]);
    }
    let mut undefined = Sheet::new("Not defined", &["joint", "why"]);
    for (j, why) in &b.undefined {
        undefined
            .rows
            .push(vec![j.clone().into(), why.clone().into()]);
    }
    let mut notes = Sheet::new("Notes", &["note"]);
    for n in &b.notes {
        notes.rows.push(vec![n.clone().into()]);
    }
    vec![bill, parts, undefined, notes]
}

/// The bill as an XLSX workbook ([`sheets`]).
pub fn workbook(b: &StructureBom) -> Result<Vec<u8>, String> {
    crate::xlsx::write(&sheets(b))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The report's section "Weight and quantities" (HTML): which self-weight
/// the run took, the totals per kind, the bill, the joints not defined and
/// the notes (those NOT ON THE SAFE SIDE marked).
pub fn report_section(b: &StructureBom, counted: bool) -> String {
    let mut h = String::from("<h2>Weight and quantities</h2>\n");
    let _ = writeln!(
        h,
        "<p>{}</p>",
        esc(&if counted {
            format!(
                "The self-weight of this run is the counted mass: {:.0} kg ({}× the members' \
                 steel); the cases whose self-weight factor is below 1 took the light count, \
                 {:.0} kg.",
                b.mass,
                b.factor().map_or("—".into(), |f| format!("{f:.3} ")),
                b.light
            )
        } else if b.every_joint_defined() {
            "Every joint is defined, but this run kept the dead-load factor (see the notes \
             above)."
                .to_owned()
        } else {
            format!(
                "Not every joint is defined ({} not): the run kept the members' steel × the \
                 dead-load factor; the parts below are those defined.",
                b.undefined.len()
            )
        })
    );
    h.push_str(
        "<table><tr><th>Parts</th><th>Count</th><th>Mass kg</th><th>Light count kg</th></tr>\n",
    );
    for t in &b.totals {
        let _ = writeln!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{:.1}</td>\
             <td class=\"n\">{:.1}</td></tr>",
            t.kind.plural(),
            t.count,
            t.mass,
            t.light
        );
    }
    let _ = writeln!(
        h,
        "<tr><th>Total</th><td></td><th class=\"n\">{:.1}</th><th class=\"n\">{:.1}</th></tr>\
         </table>",
        b.mass, b.light
    );
    h.push_str(
        "<table><tr><th>Kind</th><th>Item</th><th>Count</th><th>Quantity</th>\
         <th>Mass kg</th></tr>\n",
    );
    for r in &b.rows {
        let q = match r.kind {
            PartKind::Bolt => format!("{:.0} pcs", r.quantity),
            PartKind::Plate | PartKind::Cover => format!("{:.4} m²", r.quantity),
            PartKind::Member | PartKind::Weld => format!("{:.3} m", r.quantity),
        };
        let _ = writeln!(
            h,
            "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{:.1}</td></tr>",
            r.kind.as_str(),
            esc(&row_item(r)),
            r.count,
            q,
            r.mass
        );
    }
    h.push_str("</table>\n");
    if !b.undefined.is_empty() {
        let _ = writeln!(
            h,
            "<p>Joints not defined ({}), their parts not counted:</p><ul>",
            b.undefined.len()
        );
        for (j, why) in b.undefined.iter().take(50) {
            let _ = writeln!(h, "<li>{}: {}</li>", esc(j), esc(why));
        }
        if b.undefined.len() > 50 {
            let _ = writeln!(h, "<li>… {} more</li>", b.undefined.len() - 50);
        }
        h.push_str("</ul>\n");
    }
    h.push_str("<ul>");
    for n in &b.notes {
        let class = if n.contains("NOT ON THE SAFE SIDE") {
            "notsafe"
        } else {
            "note"
        };
        let _ = writeln!(h, "<li class=\"{class}\">{}</li>", esc(n));
    }
    h.push_str("</ul>\n");
    h
}
