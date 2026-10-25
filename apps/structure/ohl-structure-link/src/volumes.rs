// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The volumes on a structure's strings for OHL-Structure's view (owner,
//! 2026-10-25: through this crate, the one boundary): each string's
//! shapes — its insulator assembly's (`.ohli` `[[shape]]`) — placed as
//! the line places them ([`ohl_clearance::volumes::place`]), on the
//! string as it hangs in still air in the structure's own axes, and
//! handed over as triangles.

use ohl_structures::{ElectricalState, InsulatorLibrary, StructureType};

/// One shape's surface, as plain data for the view.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeMesh {
    /// The string it is on.
    pub string: String,
    /// The shape's name.
    pub name: String,
    /// Grounded, electrified or insulated.
    pub state: ElectricalState,
    /// Its surface, structure axes (z up), m.
    pub triangles: Vec<[[f64; 3]; 3]>,
}

/// Segments around a curved shape's surface.
const SEGMENTS: usize = 16;

/// The volumes of `structure`'s strings, `strings` giving each string's
/// top and wire point in still air (label, top, bottom; as the view draws
/// them), their shapes from the assemblies of `insulators`. A string
/// without an assembly or whose assembly has no shapes gives none. The
/// structure's own axes: x to the right of the line, so the shapes stand
/// as the line's do at a structure turned 0°.
pub fn string_volumes(
    structure: &StructureType,
    strings: &[(String, [f64; 3], [f64; 3])],
    insulators: &InsulatorLibrary,
) -> Vec<VolumeMesh> {
    let mut out = Vec::new();
    for (label, top, bottom) in strings {
        let Some(s) = structure.strings.iter().find(|s| &s.label == label) else {
            continue;
        };
        let Some(volumes) = s
            .insulator
            .as_deref()
            .and_then(|id| insulators.get(id))
            .map(|a| a.volumes.clone())
        else {
            continue;
        };
        if volumes.shapes.is_empty() {
            continue;
        }
        let at = ohl_clearance::volumes::StringAt {
            top: *top,
            bottom: *bottom,
            still: (*top, *bottom),
            strain: s.strain,
            orientation_deg: 0.0,
            wire_direction: None,
        };
        for p in ohl_clearance::volumes::place(&volumes, &volumes.shapes, &at) {
            out.push(VolumeMesh {
                string: label.clone(),
                name: p.name,
                state: p.state,
                triangles: p.convex.mesh(SEGMENTS),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example's invented assembly with a horn at each end, on a
    /// vertical string of a structure: its shapes placed along the string
    /// (the grounded one near the top, the electrified one near the
    /// bottom); a string without an assembly gives none.
    #[test]
    fn shapes_hang_on_the_strings_in_still_air() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let lib =
            InsulatorLibrary::load_dir(&root.join("examples/synthetic-line/Insulators")).unwrap();
        let id = "invented-i-volumes";
        let a = lib.get(id).unwrap();
        assert!(!a.volumes.shapes.is_empty());
        let text = format!(
            "format = \"ohl-structure\"\nversion = 2\nid = \"t\"\nname = \"t\"\nkind = \"lattice\"\n\
             [[point]]\nlabel = \"A\"\npoint = [\"0 m\", \"0 m\", \"20 m\"]\n\
             [[wire]]\nlabel = \"W\"\nset = 1\n\
             [[string]]\nlabel = \"S\"\ntop = \"A\"\nwire = \"W\"\ninsulator = \"{id}\"\n\
             [[string]]\nlabel = \"bare\"\ntop = \"A\"\nwire = \"W\"\nlength = \"1 m\"\n"
        );
        let st = <StructureType as ohl_structures::LibraryItem>::parse(
            std::path::Path::new("t.ohls"),
            &text,
        )
        .unwrap();
        let len = a.length.si();
        let strings = vec![
            ("S".to_owned(), [0.0, 0.0, 20.0], [0.0, 0.0, 20.0 - len]),
            ("bare".to_owned(), [0.0, 0.0, 20.0], [0.0, 0.0, 19.0]),
        ];
        let v = string_volumes(&st, &strings, &lib);
        assert!(!v.is_empty());
        assert!(v.iter().all(|m| m.string == "S" && !m.triangles.is_empty()));
        let mid_z = |m: &VolumeMesh| {
            let n = (m.triangles.len() * 3) as f64;
            m.triangles.iter().flatten().map(|p| p[2]).sum::<f64>() / n
        };
        for m in &v {
            let z = mid_z(m);
            assert!(
                z <= 20.0 + 1.0 && z >= 20.0 - len - 1.0,
                "{} at {z}",
                m.name
            );
        }
        let grounded: Vec<f64> = v
            .iter()
            .filter(|m| m.state == ElectricalState::Grounded)
            .map(mid_z)
            .collect();
        let live: Vec<f64> = v
            .iter()
            .filter(|m| m.state == ElectricalState::Electrified)
            .map(mid_z)
            .collect();
        if let (Some(g), Some(l)) = (grounded.first(), live.first()) {
            assert!(g > l, "the grounded shape above the live one");
        }
    }
}
