// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Split trees ([`crate::trees`], phase 11): the same versions as flat
//! trees — states, diffs, merges, conflicts — at the cost of what changed.

use super::*;
use crate::annotations::{Anchor, Annotation, AnnotationKind};
use crate::versions::{Choices, Side, State};

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

fn note(id: &str, x: f64) -> Command {
    Annotation {
        id: id.into(),
        kind: AnnotationKind::Note,
        tool: String::new(),
        anchors: vec![Anchor::Point {
            at: [x, 0.0, 0.0],
            what: String::new(),
        }],
        text: format!("note {id}"),
        author: "ana".into(),
        time: String::new(),
        last: None,
    }
    .put()
}

/// The example line with `notes` notes (so that the trie has levels), its
/// first revision; `split`: split trees.
fn project(name: &str, split: bool, notes: usize) -> Session {
    let dir = std::env::temp_dir().join(format!("ohl-trees-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.set_split_trees(split);
    s.import(&example(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    let commands: Vec<Command> = (0..notes)
        .map(|k| note(&format!("n{k}"), k as f64))
        .collect();
    if !commands.is_empty() {
        s.apply("ana", Command::Batch { commands }).unwrap();
    }
    s.commit("ana", "first").unwrap();
    s
}

fn shift(s: &mut Session, id: &str, dx: f64) -> Result<Applied, SessionError> {
    let p = s.line().structure(id).unwrap().position;
    s.apply(
        "ana",
        Command::MoveStructure {
            id: id.into(),
            position: [p[0] + dx, p[1], p[2]],
        },
    )
}

/// xorshift: the same script for both sessions.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The objects of a state, for comparing two sessions (revision ids
/// differ: their trees do).
fn objects(state: &State) -> Vec<((Kind, String), i64, Value)> {
    state
        .objects
        .iter()
        .map(|(k, (o, v))| (k.clone(), *o, (**v).clone()))
        .collect()
}

fn outcome<T>(r: &Result<T, SessionError>) -> String {
    match r {
        Ok(_) => "ok".into(),
        // Revision ids differ between the two kinds of tree.
        Err(e) => e
            .to_string()
            .split_whitespace()
            .filter(|w| !(w.len() >= 7 && w.chars().all(|c| c.is_ascii_hexdigit())))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

thread_local! {
    /// Merges planned from what differs, and conflicts met (this thread's).
    static SEEN: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

/// One step of the script on one session, in words (for comparing).
fn step(s: &mut Session, what: usize, r: &mut Rng, lines: &mut Vec<String>) -> String {
    let structures = ["T1", "T2", "T3", "T4", "T5", "T6", "T7"];
    match what {
        0..=2 => {
            let id = structures[r.below(structures.len())];
            let dx = (r.below(5) as f64) - 2.0;
            outcome(&shift(s, id, dx))
        }
        3 => {
            let k = r.below(400);
            outcome(&s.apply("ana", note(&format!("n{k}"), (k * 7) as f64)))
        }
        4 => {
            let k = r.below(400);
            outcome(&s.apply(
                "ana",
                Command::Delete {
                    kind: Kind::Annotation,
                    id: format!("n{k}"),
                    confirm: false,
                },
            ))
        }
        5 | 6 => outcome(&s.commit("ana", &format!("c{}", r.next() % 1000))),
        7 => {
            let name = format!("l{}", lines.len());
            let o = outcome(&s.create_line("ana", &name, s.current_line().to_owned().as_str()));
            if o == "ok" {
                lines.push(name);
            }
            o
        }
        8 => {
            let name = lines[r.below(lines.len())].clone();
            outcome(&s.select_line(&name))
        }
        9 | 10 => {
            let other = lines[r.below(lines.len())].clone();
            let first = outcome(&s.commit("ana", "before merging"));
            let plan = s.merge_plan(&other, &Choices::new());
            let mut choices = Choices::new();
            let mut words = outcome(&plan);
            if let Ok(p) = &plan {
                for c in &p.conflicts {
                    words.push_str(&format!(" {}", c.key));
                    choices.insert(
                        c.key.clone(),
                        if r.below(2) == 0 {
                            Side::Ours
                        } else {
                            Side::Theirs
                        },
                    );
                }
                words.push_str(&format!(" {:?}", p.kind));
                SEEN.with(|x| {
                    let (l, c) = x.get();
                    x.set((l + usize::from(p.local.is_some()), c + p.conflicts.len()));
                });
                words.push_str(&format!(" changes {}", p.changes.objects.len()));
            }
            let merged = s.merge("ana", &other, Some("m"), &choices);
            format!("{first} / {words} / {}", outcome(&merged))
        }
        _ => {
            let other = lines[r.below(lines.len())].clone();
            let plan = s.bring_in_plan(&other, &Choices::new());
            let mut choices = Choices::new();
            let mut words = outcome(&plan);
            if let Ok((conflicts, problem)) = &plan {
                for c in conflicts {
                    words.push_str(&format!(" {}", c.key));
                    choices.insert(
                        c.key.clone(),
                        if r.below(2) == 0 {
                            Side::Ours
                        } else {
                            Side::Theirs
                        },
                    );
                }
                words.push_str(&format!(" {problem:?}"));
            }
            let done = s.bring_in("ana", &other, &choices);
            format!("{words} / {}", outcome(&done))
        }
    }
}

/// The same random script of edits, commits, project lines, merges and
/// bringing in, on a session writing flat trees and one writing split
/// trees: the same answers, conflicts, states and differences at every
/// step.
#[test]
fn split_trees_give_the_same_versions_as_flat_ones() {
    SEEN.with(|x| x.set((0, 0)));
    for seed in 1..=5u64 {
        let mut flat = project(&format!("flat-{seed}"), false, 300);
        let mut split = project(&format!("split-{seed}"), true, 300);
        let (mut ra, mut rb) = (Rng(seed * 7919), Rng(seed * 7919));
        let (mut la, mut lb) = (vec!["main".to_owned()], vec!["main".to_owned()]);
        for n in 0..150 {
            let what = ra.below(12);
            assert_eq!(what, rb.below(12));
            let a = step(&mut flat, what, &mut ra, &mut la);
            let b = step(&mut split, what, &mut rb, &mut lb);
            assert_eq!(a, b, "seed {seed}, step {n} ({what})");
            assert_eq!(flat.current_line(), split.current_line());
            assert_eq!(
                objects(&flat.working_state().unwrap()),
                objects(&split.working_state().unwrap()),
                "seed {seed}, step {n}"
            );
            assert_eq!(
                flat.status().unwrap().changes,
                split.status().unwrap().changes,
                "seed {seed}, step {n}"
            );
            for l in &la {
                assert_eq!(
                    flat.has_uncommitted(l).unwrap(),
                    split.has_uncommitted(l).unwrap()
                );
            }
        }
        // Every branch's revision reads back the same, and so do the
        // differences between them.
        for l in &la {
            let (a, b) = (
                flat.branch_revision(l).unwrap(),
                split.branch_revision(l).unwrap(),
            );
            if let (Some(a), Some(b)) = (a, b) {
                assert_eq!(
                    objects(&flat.revision_state(&a).unwrap()),
                    objects(&split.revision_state(&b).unwrap())
                );
                assert_eq!(
                    flat.diff_revisions("main", l).unwrap(),
                    split.diff_revisions("main", l).unwrap()
                );
                assert_eq!(
                    flat.ahead_behind("main", l).unwrap(),
                    split.ahead_behind("main", l).unwrap()
                );
            }
        }
    }
    // The script met merges worked out from what differs, and conflicts.
    let (local, conflicts) = SEEN.with(std::cell::Cell::get);
    assert!(
        local >= 5 && conflicts >= 5,
        "{local} local merges, {conflicts} conflicts"
    );
}

/// A commit writes what changed: one object, the nodes on its path and
/// the root — not the line again.
#[test]
fn a_commit_writes_what_changed() {
    let mut s = project("cost", true, 500);
    let before = s.conn.blob_hashes().unwrap().len();
    shift(&mut s, "T3", 1.0).unwrap();
    // The structure and what follows it (its alignment point).
    let changed = s.status().unwrap().changes.objects.len();
    assert!((1..=2).contains(&changed));
    s.commit("ana", "T3 east").unwrap();
    let written = s.conn.blob_hashes().unwrap().len() - before;
    // The objects, two levels of nodes each at most, the root.
    assert!(written <= 3 * changed + 1, "{written} blobs written");
    // A flat tree writes the whole list again (and the object).
    let mut f = project("cost-flat", false, 500);
    let before = f.conn.blob_hashes().unwrap().len();
    shift(&mut f, "T3", 1.0).unwrap();
    f.commit("ana", "T3 east").unwrap();
    assert_eq!(f.conn.blob_hashes().unwrap().len() - before, changed + 1);
    // Nothing to commit is told on split trees too.
    assert!(s.commit("ana", "again").is_err());
}

/// A commit undone with the unit of work it was part of (a merge refused
/// after it) leaves nothing half-written behind: the next commit writes
/// what its tree needs, and every revision reads back.
#[test]
fn a_commit_rolled_back_leaves_no_missing_blob() {
    let mut s = project("rolled", true, 300);
    shift(&mut s, "T2", 1.0).unwrap();
    let r: Result<(), SessionError> = s.unit(|s| {
        s.commit("ana", "kept for a moment")?;
        Err(refused("a later step failed"))
    });
    assert!(r.is_err());
    // The line is put back (the edit was before the unit: still there).
    let rev = s.commit("ana", "T2 east").unwrap();
    let state = s.revision_state(&rev.id).unwrap();
    assert_eq!(objects(&state), objects(&s.working_state().unwrap()));
    assert!(s.status().unwrap().changes.is_empty());
}

/// A model with flat revisions (imported before phase 11) going on with
/// split ones: nothing to commit is told across the two kinds, the
/// differences between them are exact, a bundle carries the split nodes.
#[test]
fn flat_and_split_revisions_mix() {
    let mut s = project("mixed", false, 200);
    let flat_rev = s.head_revision().unwrap().unwrap();
    s.set_split_trees(true);
    assert!(s.status().unwrap().changes.is_empty());
    assert!(!s.has_uncommitted("main").unwrap());
    assert!(s.commit("ana", "nothing").is_err());
    shift(&mut s, "T4", 2.0).unwrap();
    let split_rev = s.commit("ana", "T4 east").unwrap();
    let d = s.diff_revisions(&flat_rev, &split_rev.id).unwrap();
    assert_eq!(d.objects.len(), 1);
    assert_eq!(d.objects[0].id, "T4");
    // A bundle of main carries both kinds; another copy of the project
    // takes it in, reads it back.
    let bundle = s.export_bundle("main").unwrap();
    let mut other = project("mixed-other", false, 200);
    other
        .import_bundle("ana", &bundle, Some("from the network"))
        .unwrap();
    assert_eq!(
        objects(&other.revision_state(&split_rev.id).unwrap()),
        objects(&s.working_state().unwrap())
    );
    // A bundle without one of its nodes is refused, nothing written.
    let mut cut = s.export_bundle("main").unwrap();
    let node = cut
        .blobs
        .iter()
        .find(|(_, t)| t.starts_with("{\"c\":") || t.starts_with("{\"e\":"))
        .map(|(h, _)| h.clone())
        .expect("a node in the bundle");
    cut.blobs.remove(&node);
    let mut third = project("mixed-third", false, 0);
    assert!(third.import_bundle("ana", &cut, Some("cut")).is_err());
}

/// A merge worked out from what differs reads only that: its plan says
/// which objects, and carries it out as one command validated where it
/// changes; a merge that would make the line invalid is refused, nothing
/// kept.
#[test]
fn merges_from_what_differs() {
    let mut s = project("local", true, 300);
    s.create_line("ana", "alt", "main").unwrap();
    s.select_line("alt").unwrap();
    shift(&mut s, "T5", 1.5).unwrap();
    s.commit("ana", "T5 east").unwrap();
    s.select_line("main").unwrap();
    shift(&mut s, "T6", -1.0).unwrap();
    s.commit("ana", "T6 west").unwrap();
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    let keys = plan.local.clone().expect("worked out locally");
    // The two structures and what follows them (alignment points).
    let ids: Vec<&str> = keys
        .iter()
        .filter(|(k, _)| *k == Kind::Structure)
        .map(|(_, id)| id.as_str())
        .collect();
    assert_eq!(ids, ["T5", "T6"]);
    assert!(
        keys.iter()
            .all(|(k, _)| matches!(k, Kind::Structure | Kind::AlignmentPoint)),
        "{keys:?}"
    );
    assert!(plan.changes.objects.iter().any(|c| c.id == "T5"));
    assert!(!plan.changes.objects.iter().any(|c| c.id == "T6"));
    let before = s.line().structure("T6").unwrap().position;
    s.merge("ana", "alt", None, &Choices::new()).unwrap();
    assert_eq!(s.line().structure("T6").unwrap().position, before);
    assert!(s.status().unwrap().changes.is_empty());
    // A flat session: no local plan.
    let f = project("local-flat", false, 0);
    assert!(
        f.merge_plan("main", &Choices::new())
            .unwrap()
            .local
            .is_none()
    );
}

/// Phase 12's second item: a branch's base kept as a split tree is read
/// along what differs from the main line — the same objects (places,
/// versions, JSON, library files) as the revision read whole, and every
/// line the same after the project is opened again.
#[test]
fn a_branchs_base_is_read_along_what_differs() {
    let mut s = project("bases", true, 200);
    let path = std::env::temp_dir()
        .join(format!("ohl-trees-{}-bases", std::process::id()))
        .join("p.ohl");
    let r1 = s.head_revision().unwrap().unwrap();
    s.create_line("ana", "a", "main").unwrap();
    shift(&mut s, "T2", 1.5).unwrap();
    s.put_library_file("ana", "weather", "cases", "[cases]\n")
        .unwrap();
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Annotation,
            id: "n5".into(),
            confirm: true,
        },
    )
    .unwrap();
    s.apply("ana", note("n999", 3.0)).unwrap();
    let r2 = s.commit("ana", "second").unwrap().id;
    s.create_line("ana", "b", "main").unwrap();
    s.create_line("ana", "c", &r1).unwrap();
    s.with_line("b", |s| shift(s, "T6", -0.5).map(|_| ()))
        .unwrap();
    // Main moves on, uncommitted: the bases still come out as stored.
    shift(&mut s, "T4", 2.0).unwrap();
    s.put_library_file("ana", "weather", "more", "[cases]\n# more\n")
        .unwrap();
    let main = s.line_objects_named("main").unwrap().clone();
    for rev in [&r1, &r2] {
        let whole = crate::lines::objects_of_state(s.revision_state(rev).unwrap());
        let along = s.objects_at(rev, "main", &main).unwrap().expect("split");
        assert_eq!(along.len(), whole.len());
        for (k, o) in &whole {
            let a = &along[k];
            assert_eq!(
                (a.ord, a.version, &*a.value),
                (o.ord, o.version, &*o.value),
                "{k:?}"
            );
        }
    }
    type Held = BTreeMap<String, Vec<((Kind, String), i64, Value)>>;
    let held = |s: &Session| -> Held {
        ["main", "a", "b", "c"]
            .iter()
            .map(|n| {
                let o = s.line_objects_named(n).unwrap();
                (
                    n.to_string(),
                    o.iter()
                        .map(|(k, o)| (k.clone(), o.ord, (*o.value).clone()))
                        .collect(),
                )
            })
            .collect()
    };
    let before = held(&s);
    drop(s);
    let again = Session::open(&path).unwrap();
    assert_eq!(held(&again), before);
}

/// A project line made ready on another thread ([`crate::lines::LineSource`],
/// the exports' thread of a network's server) is the line `create_line`
/// makes itself from the same revision: the same objects, line, spatial
/// and reference indexes, and after the same command and commit the same
/// tree. An offer from another revision is not taken, and dropped.
#[test]
fn a_line_made_ready_elsewhere_is_the_line_made_here() {
    for split in [true, false] {
        let mut s = project(&format!("prepared-{split}"), split, 40);
        let first = s.head_revision().unwrap().unwrap();
        shift(&mut s, "T2", 3.0).unwrap();
        s.commit("ana", "second").unwrap();
        let second = s.head_revision().unwrap().unwrap();
        shift(&mut s, "T3", 1.0).unwrap();
        // Made here, then from a line made ready on another thread.
        s.create_line("ana", "here", &first).unwrap();
        let source = s.line_source(&first).unwrap();
        assert_eq!(source.revision(), first);
        let ready = std::thread::spawn(move || source.prepare())
            .join()
            .unwrap()
            .unwrap();
        s.offer_line(ready);
        s.create_line("ana", "ready", &first).unwrap();
        assert!(s.offered.is_none());
        let (a, b) = (&s.others["here"], &s.others["ready"]);
        assert_eq!(a.objects, b.objects);
        assert_eq!(a.line, b.line);
        assert_eq!(a.spatial, b.spatial);
        assert_eq!(
            b.index.as_ref().unwrap(),
            &ohl_line::index::LineIndex::of(&a.line)
        );
        assert_eq!(s.line_tree("here").unwrap(), s.line_tree("ready").unwrap());
        let mut trees = Vec::new();
        for name in ["here", "ready"] {
            s.select_line(name).unwrap();
            shift(&mut s, "T4", 2.0).unwrap();
            trees.push(s.commit("ana", &format!("on {name}")).unwrap().tree);
        }
        assert_eq!(trees[0], trees[1], "split {split}");
        s.select_line(crate::versions::MAIN).unwrap();
        // Offered from the first revision, a line made from the second
        // works it out itself; the offer goes.
        let source = s.line_source(&first).unwrap();
        s.offer_line(source.prepare().unwrap());
        s.create_line("ana", "other", &second).unwrap();
        assert!(s.offered.is_none());
        s.create_line("ana", "other-here", &second).unwrap();
        assert_eq!(s.others["other"].line, s.others["other-here"].line);
        assert_ne!(s.others["other"].line, s.others["here"].line);
    }
}
