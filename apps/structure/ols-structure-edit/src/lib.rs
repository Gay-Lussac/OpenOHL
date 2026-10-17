// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The editing of a structure file (`.olss`), without a window: every
//! edit of the structure application (and of the line application's
//! pylon workspace) as a function on the file's text ([`toml_edit`], so
//! comments and layout are kept), a file with an undo and redo of whole
//! edits ([`StructureFile`]), the positions of what is drawn and picked
//! ([`Geometry`]), and what a structure still lacks for each level
//! ([`missing()`]); the edits of its load-case recipes
//! ([`load_cases`](mod@load_cases)); and the body tools: generators
//! ([`body`], [`generators`], [`mod@rebuild`]), mirror symmetry
//! ([`symmetry`]) and member groups ([`mod@groups`]).
//!
//! Axes are the structure's own: x right of the line, y ahead, z up,
//! 0;0 at the centre.

pub mod body;
pub mod edits;
pub mod file;
pub mod generators;
pub mod geometry;
pub mod groups;
pub mod load_cases;
pub mod missing;
pub mod rebuild;
pub mod symmetry;

pub use body::{
    ArmShape, Bracing, CopyFace, Crossarm, Face, Generator, Kind, Mirror, Panel, Panels, Peak,
    Symmetry,
};
pub use edits::{
    Removal, StringChange, add_item, add_joint, add_member, add_point, add_point_at_joint, add_set,
    add_strings, assembly_text, delete_string, drop_set, item_fields, item_ids, move_set, move_to,
    new_string, next_id, removal, remove, remove_extension, remove_item, set_counterweight,
    set_extension, set_item_field, set_member_k, set_member_kind, set_member_section, set_set,
    set_string, set_support, set_top_field,
};
pub use file::StructureFile;
pub use generators::{Body, generate};
pub use geometry::{Geometry, Handle, compass_radius, geometry, pick, polar, position};
pub use groups::{Group, groups, set_group, set_group_kind, set_group_section};
pub use load_cases::{
    CASE_FIELDS, CaseText, LOAD_FIELDS, add_load, add_load_case, load_cases, merge_load_cases,
    remove_load, remove_load_case, set_load_case_field, set_load_field,
};
pub use missing::{Missing, Need, Place, missing, next_level};
pub use rebuild::{Report, apply, rebuild, rebuild_all, remove_generator};

#[cfg(test)]
mod body_tests;
#[cfg(test)]
mod tests;
