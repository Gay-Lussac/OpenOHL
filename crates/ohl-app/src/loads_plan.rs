// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a run of Checks › Structure loads computes (owner, 2026-11-15:
//! "the load cases on the structure should respect the groups defined in
//! the load case per group if there are some … it should have a list of
//! groups that are to be run and then the loadcases for the structures not
//! in these groups under"): the window's **choice** ([`Choice`]: the
//! groups run, the load cases run for the structures in no group, the
//! structures ticked) and from it and the line's groups
//! ([`ohl_line::CaseSets`]) the **plan** ([`plan`]): per structure,
//! exactly the load cases it is computed for.
//!
//! - A structure in groups that are all run: every case it takes (its
//!   groups' union, the cases' own lists — picks, exclusions, rules).
//! - In a group run and a group not run: the cases of the groups run (and
//!   of the own lists picking it or whose rules give it), less those it
//!   is excluded from — said in the summary ([`Plan::partly`]).
//! - In groups none of which is run: not computed ([`Plan::skipped`]).
//! - In no group: the cases it takes (every case, less its own-list
//!   exclusions) that are ticked for the structures in no group — a
//!   choice of this run, kept with the window's setup in the project; the
//!   line's groups are not changed.
//! - A structure on no alignment is left out unless ticked (owner,
//!   2026-11-15: "put the structures outside of the alignment by default
//!   out of the calculations and all at the bottom of the list"); one on
//!   an alignment is computed unless unticked.
//!
//! Nothing is left out without being said: a structure not computed is
//! in [`Plan::skipped`] with why, one taking no case in [`Plan::none`].

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The window's setup, kept in the project ([`crate::loads::SETTING`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Choice {
    /// **Use local conditions**.
    #[serde(default)]
    pub local: bool,
    /// **Structures flexible** (calculation level 3, plan of 2026-11-15,
    /// phase 4; in active development): the loads and usage the larger
    /// of the rigid and the coupled solve's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub level3: bool,
    /// The structure groups **not** run, by name (a group is run by
    /// default, also one added later).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups_off: Vec<String>,
    /// The load cases run for the structures in no group (`None`: every
    /// case, also cases added later to the library).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alone_cases: Option<Vec<String>>,
    /// The structures on an alignment **not** computed (unticked), by id
    /// (computed by default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structures_off: Vec<String>,
    /// The structures on no alignment computed (ticked), by id (left out
    /// by default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structures_on: Vec<String>,
}

impl Choice {
    /// Whether group `name` is run.
    pub fn runs_group(&self, name: &str) -> bool {
        !self.groups_off.iter().any(|g| g == name)
    }

    /// Whether structure `id` is ticked (`aligned`: it is on an alignment
    /// — ticked unless unticked; else unticked unless ticked).
    pub fn ticks(&self, id: &str, aligned: bool) -> bool {
        if aligned {
            !self.structures_off.iter().any(|s| s == id)
        } else {
            self.structures_on.iter().any(|s| s == id)
        }
    }

    /// Structure `id` ticked (`on`) or not.
    pub fn tick(&mut self, id: &str, aligned: bool, on: bool) {
        self.structures_off.retain(|s| s != id);
        self.structures_on.retain(|s| s != id);
        match (aligned, on) {
            (true, false) => self.structures_off.push(id.to_owned()),
            (false, true) => self.structures_on.push(id.to_owned()),
            _ => {}
        }
    }
}

/// Said of a structure on no alignment left out (the default).
pub const OFF_ALIGNMENT: &str = "not on an alignment, unticked";

/// The structures on an alignment (a station on one, as the Load cases
/// form's *Applies to* says it). Expected cost: the stations of every
/// alignment (milliseconds for a long line).
pub fn on_alignment(line: &ohl_line::Line) -> BTreeSet<String> {
    line.structure_facts_with_stations()
        .into_iter()
        .filter(|(_, f)| !f.stations.is_empty())
        .map(|(id, _)| id)
        .collect()
}

/// What a run computes: per structure, the load cases.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Per structure computed (by id), the load cases of the library it
    /// is computed for, in the library's order.
    pub cases: BTreeMap<String, Vec<String>>,
    /// The structures computed, in the line's order.
    pub order: Vec<String>,
    /// The load cases to solve (those of some structure), in the library's
    /// order.
    pub solve: Vec<String>,
    /// The structures not computed, with why, in the line's order.
    pub skipped: Vec<(String, String)>,
    /// The structures in a group run and a group not run (computed for the
    /// cases of the groups run), with the groups not run.
    pub partly: Vec<(String, Vec<String>)>,
    /// The structures taking no load case (their groups tick none).
    pub none: Vec<String>,
}

impl Plan {
    /// Whether structure `id` is computed for load case `case` (a set
    /// name: a case of several wind directions by its own name).
    pub fn computes(&self, id: &str, case: &str) -> bool {
        self.cases
            .get(id)
            .is_some_and(|c| c.iter().any(|x| x == case))
    }

    /// How many (structure, load case) pairs it computes.
    pub fn pairs(&self) -> usize {
        self.cases.values().map(Vec::len).sum()
    }
}

/// The plan of `choice` on `line` (its groups resolved: `sets`) with the
/// load case library's case names `library`; `scope`: the structures of
/// the route worked on (View › Work on route), others not computed;
/// `aligned`: the structures on an alignment ([`on_alignment`]).
/// Expected cost: one pass over the structures and their groups.
pub fn plan(
    line: &ohl_line::Line,
    sets: &ohl_line::CaseSets,
    library: &[String],
    choice: &Choice,
    scope: Option<&BTreeSet<String>>,
    aligned: &BTreeSet<String>,
) -> Plan {
    let mut out = Plan::default();
    // The cases each group gives, by name (own lists left out: they are
    // not groups for "in no group").
    let given: BTreeMap<&str, &[String]> = line
        .structure_groups
        .iter()
        .filter(|g| !g.is_own_list())
        .map(|g| (g.name.as_str(), g.load_cases.as_slice()))
        .collect();
    for s in &line.structures {
        let id = s.id.as_str();
        if scope.is_some_and(|on| !on.contains(id)) {
            out.skipped
                .push((id.to_owned(), "not on the route worked on".to_owned()));
            continue;
        }
        if sets.takes_none(id) {
            out.none.push(id.to_owned());
            continue;
        }
        let on_alignment = aligned.contains(id);
        if !choice.ticks(id, on_alignment) {
            out.skipped.push((
                id.to_owned(),
                if on_alignment {
                    "unticked".to_owned()
                } else {
                    OFF_ALIGNMENT.to_owned()
                },
            ));
            continue;
        }
        let groups = sets.groups_of(id);
        let cases: Vec<String> = if groups.is_empty() {
            library
                .iter()
                .filter(|c| sets.takes(id, c) && crate::tick_list::ticked(&choice.alone_cases, c))
                .cloned()
                .collect()
        } else {
            let (on, off): (Vec<&str>, Vec<&str>) =
                groups.iter().partition(|g| choice.runs_group(g));
            if on.is_empty() {
                out.skipped.push((
                    id.to_owned(),
                    format!("its groups not run: {}", off.join(", ")),
                ));
                continue;
            }
            if !off.is_empty() {
                out.partly
                    .push((id.to_owned(), off.iter().map(|g| (*g).to_owned()).collect()));
            }
            // The cases of the groups run and of the own lists picking it
            // or giving it by their rules; the exclusions are in `takes`.
            let allowed: BTreeSet<&str> = on
                .iter()
                .flat_map(|g| given.get(g).copied().unwrap_or_default())
                .chain(sets.picked_for(id))
                .chain(sets.ruled_into(id))
                .map(String::as_str)
                .collect();
            library
                .iter()
                .filter(|c| sets.takes(id, c) && allowed.contains(c.as_str()))
                .cloned()
                .collect()
        };
        if cases.is_empty() {
            out.skipped.push((
                id.to_owned(),
                if groups.is_empty() {
                    "in no group: no load case ticked for it".to_owned()
                } else {
                    "no load case of the library from its groups run".to_owned()
                },
            ));
            continue;
        }
        out.order.push(id.to_owned());
        out.cases.insert(id.to_owned(), cases);
    }
    out.solve = library
        .iter()
        .filter(|c| out.cases.values().any(|v| v.contains(c)))
        .cloned()
        .collect();
    out
}

/// Who gives structure `id` load case `case`, in words: the groups ticking
/// it, *no group* (every case), the case's own list (picked, its rule).
pub fn given_by(sets: &ohl_line::CaseSets, id: &str, case: &str) -> String {
    let why = sets.why(id, case);
    let mut by: Vec<String> = why.by.iter().map(|g| g.group.clone()).collect();
    if why.no_group {
        by.push("no group (every case)".to_owned());
    }
    if why.picked_here {
        by.push(format!("{case} (own list): picked"));
    }
    if why.rule_here {
        by.push(format!("{case} (own list): its rule"));
    }
    by.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example line (T1–T7); group "a" picks T1, T2 with case "x";
    /// group "b" picks T2, T3 with "y"; T4–T7 in no group; "z"'s own
    /// list excludes T5.
    fn line() -> ohl_line::Line {
        let mut l = ohl_line::Line::load(
            &crate::tests::root().join("examples/synthetic-line/synthetic-line.ohl.json"),
        )
        .unwrap();
        let mut a = ohl_line::StructureGroup::new("a");
        a.structures = vec!["T1".into(), "T2".into()];
        a.load_cases = vec!["x".into()];
        let mut b = ohl_line::StructureGroup::new("b");
        b.structures = vec!["T2".into(), "T3".into()];
        b.load_cases = vec!["y".into()];
        let mut z = ohl_line::StructureGroup::own_list("z");
        z.excluded = vec!["T5".into()];
        l.structure_groups = vec![a, b, z];
        l
    }

    /// Every structure on an alignment.
    fn all(l: &ohl_line::Line) -> BTreeSet<String> {
        l.structures.iter().map(|s| s.id.clone()).collect()
    }

    fn library() -> Vec<String> {
        ["x", "y", "z"].map(str::to_owned).to_vec()
    }

    /// Every group run, every case for those in no group: each structure
    /// computed for exactly the cases the groups model gives it.
    #[test]
    fn every_group_run_computes_exactly_the_cases_taken() {
        let l = line();
        let sets = l.case_sets();
        let p = plan(&l, &sets, &library(), &Choice::default(), None, &all(&l));
        for s in &l.structures {
            let want: Vec<String> = library()
                .into_iter()
                .filter(|c| sets.takes(&s.id, c))
                .collect();
            assert_eq!(p.cases.get(&s.id), Some(&want), "{}", s.id);
        }
        assert_eq!(p.cases["T5"], ["x", "y"], "excluded from z");
        assert_eq!(p.solve, library());
        assert!(p.skipped.is_empty() && p.partly.is_empty());
    }

    /// Group "b" not run: T3 (only in b) not computed, said; T2 (in a and
    /// b) computed for a's case only, said; the others as before.
    #[test]
    fn a_group_not_run_skips_its_structures_and_says_so() {
        let l = line();
        let sets = l.case_sets();
        let choice = Choice {
            groups_off: vec!["b".into()],
            ..Choice::default()
        };
        let p = plan(&l, &sets, &library(), &choice, None, &all(&l));
        assert_eq!(p.cases["T2"], ["x"]);
        assert!(!p.cases.contains_key("T3"));
        assert_eq!(p.skipped, [("T3".into(), "its groups not run: b".into())]);
        assert_eq!(p.partly, [("T2".into(), vec!["b".to_owned()])]);
        assert_eq!(p.cases["T4"], ["x", "y", "z"]);
    }

    /// The cases ticked for the structures in no group change only theirs;
    /// a structure unticked is not computed, said.
    #[test]
    fn the_cases_ticked_for_structures_in_no_group_change_only_theirs() {
        let l = line();
        let sets = l.case_sets();
        let choice = Choice {
            alone_cases: Some(vec!["z".into()]),
            ..Choice::default()
        };
        let p = plan(&l, &sets, &library(), &choice, None, &all(&l));
        assert_eq!(p.cases["T4"], ["z"]);
        // T5 is excluded from z: nothing left, said.
        assert!(!p.cases.contains_key("T5"));
        assert!(
            p.skipped
                .iter()
                .any(|(s, why)| s == "T5" && why.contains("no load case"))
        );
        assert_eq!(p.cases["T1"], ["x"]);
        assert_eq!(p.cases["T2"], ["x", "y"]);
        let mut choice = Choice::default();
        choice.tick("T4", true, false);
        let p = plan(&l, &sets, &library(), &choice, None, &all(&l));
        assert!(!p.cases.contains_key("T4"));
        assert!(
            p.skipped
                .iter()
                .any(|(s, why)| s == "T4" && why == "unticked")
        );
    }

    /// A structure on no alignment: left out unless ticked, said (owner
    /// 2026-11-15); ticked, computed as the others.
    #[test]
    fn structures_on_no_alignment_are_left_out_unless_ticked() {
        let l = line();
        let sets = l.case_sets();
        let mut aligned = all(&l);
        aligned.remove("T1");
        aligned.remove("T4");
        let mut choice = Choice::default();
        let p = plan(&l, &sets, &library(), &choice, None, &aligned);
        assert!(!p.cases.contains_key("T1") && !p.cases.contains_key("T4"));
        assert_eq!(
            p.skipped,
            [
                ("T1".to_owned(), OFF_ALIGNMENT.to_owned()),
                ("T4".to_owned(), OFF_ALIGNMENT.to_owned())
            ]
        );
        choice.tick("T4", false, true);
        let p = plan(&l, &sets, &library(), &choice, None, &aligned);
        assert_eq!(p.cases["T4"], ["x", "y", "z"]);
        assert_eq!(choice.structures_on, ["T4"]);
    }

    /// Who gave each case, in words.
    #[test]
    fn given_by_names_the_groups() {
        let l = line();
        let sets = l.case_sets();
        assert_eq!(given_by(&sets, "T2", "y"), "b");
        assert_eq!(given_by(&sets, "T4", "x"), "no group (every case)");
    }

    /// The setup kept before 2026-11-15 (`{"local": true}`) still reads.
    #[test]
    fn the_old_setup_still_reads() {
        let c: Choice = serde_json::from_str("{\"local\": true}").unwrap();
        assert!(c.local && c.groups_off.is_empty() && c.alone_cases.is_none());
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(text, "{\"local\":true}");
    }
}
